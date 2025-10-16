use std::{error::Error, net::SocketAddr, sync::Arc};

use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

pub struct Server {
    config: ServerConfig,
    cert: CertificateDer<'static>,
    /// Max connections to allow, or we let it keep growing until something breaks.
    max_connections: Option<usize>,
}

impl Server {
    pub fn new(
        max_connections: Option<usize>,
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        let (config, cert) = Self::configure_server()?;
        Ok(Server {
            config,
            cert,
            max_connections,
        })
    }

    fn configure_server()
    -> Result<(ServerConfig, CertificateDer<'static>), Box<dyn Error + Send + Sync + 'static>> {
        // Code taken from: https://github.com/quinn-rs/quinn/blob/e05a8e5a7e4d068a98ec861ed722999f90f28a43/quinn/examples/common/mod.rs
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_der = CertificateDer::from(cert.cert);
        let priv_key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());

        let mut server_config =
            ServerConfig::with_single_cert(vec![cert_der.clone()], priv_key.into())?;
        let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
        transport_config.max_concurrent_uni_streams(0_u8.into());

        Ok((server_config, cert_der))
    }

    pub async fn serve(
        &self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn Error + Sync + Send + 'static>> {
        // Don't mind cloning here because we should only do this on startup.
        let endpoint = Endpoint::server(self.config.clone(), addr)?;

        // Start iterating over incoming connections.
        while let Some(conn) = endpoint.accept().await {
            // Going to keep each connection in its own thread.
            let connection = conn.await?;
            tokio::spawn(async move {
                println!(
                    "Connection from remote address {}",
                    connection.remote_address()
                )
            });
        }

        Ok(())
    }
}
