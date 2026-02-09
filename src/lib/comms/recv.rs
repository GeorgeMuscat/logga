use anyhow::{Result, anyhow};

use quinn::{Endpoint, ServerConfig, VarInt};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::{net::SocketAddr, str, sync::Arc};
use tracing::info;
use uuid::Uuid;

pub struct Server {
    id: uuid::Uuid,
    config: ServerConfig,
    cert: CertificateDer<'static>,
}

impl Server {
    /// `max_connections` defaults to 1024 if unspecified
    pub fn new(max_connections: Option<VarInt>) -> Result<Self> {
        let (config, cert) = Self::configure_server(max_connections.unwrap_or(1024_u32.into()))?;
        info!("Created server");
        Ok(Server {
            id: Uuid::new_v4(),
            config,
            cert,
        })
    }

    pub fn get_cert(&self) -> &CertificateDer<'static> {
        &self.cert
    }

    fn configure_server(
        max_connections: VarInt,
    ) -> Result<(ServerConfig, CertificateDer<'static>)> {
        // Code taken from: https://github.com/quinn-rs/quinn/blob/e05a8e5a7e4d068a98ec861ed722999f90f28a43/quinn/examples/common/mod.rs
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = CertificateDer::from(cert.cert);
        let priv_key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());

        let mut server_config =
            ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;
        let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
        transport_config.max_concurrent_uni_streams(max_connections);
        transport_config.max_concurrent_bidi_streams(max_connections);

        Ok((server_config, cert_der))
    }

    pub async fn serve(&self, addr: SocketAddr) -> Result<()> {
        // Don't mind cloning here because we should only do this on startup.
        let endpoint = Endpoint::server(self.config.clone(), addr)?;
        let server_id = self.id;
        // Start iterating over incoming connections.
        while let Some(conn) = endpoint.accept().await {
            // Going to keep each connection in its own thread.

            tokio::spawn(async move {
                let connection = conn
                    .await
                    .map_err(|e| anyhow!("failed to connect: {}", e))?;
                println!(
                    "Connection from remote address {}",
                    connection.remote_address()
                );

                let (mut send, mut recv) = connection
                    .accept_bi()
                    .await
                    .map_err(|e| anyhow!("failed to open stream: {}", e))?;

                // Recv first, then send data
                let msg = recv
                    .read_to_end(usize::MAX)
                    .await
                    .map_err(|e| anyhow!("failed to read request: {}", e))?;
                // We don't escape the content sent from the client because this is a demo/poc
                println!(
                    "Server {} reporting message: {}",
                    &server_id,
                    str::from_utf8(&msg)?
                );

                // Send response
                send.write_all(format!("Hello from server {}", server_id).as_bytes())
                    .await?;
                // Tell the client we are finished writing
                send.finish()?;

                // Make sure we wait for the client to receive all the info
                send.stopped().await?;
                Ok::<(), anyhow::Error>(())
            });
        }

        Ok(())
    }
}
