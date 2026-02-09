use anyhow::Result;
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{Connection, Endpoint};
use rustls::pki_types::CertificateDer;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Client {
    // Need to track all the certs we trust.
    trusted_certs: rustls::RootCertStore,
    // The endpoint contains the ClientConfig.
    // We only need a single endpoint as it acts as both the traditional server and client.
    pub endpoint: Endpoint,
}

impl Client {
    pub fn new_no_client_auth(bind_addr: SocketAddr) -> Result<Self> {
        // This function is marked as dangerous as it does no client authentication.
        let mut endpoint = Endpoint::client(bind_addr)?;
        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(rustls::RootCertStore::empty())
            .with_no_client_auth();

        let client_config =
            quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));
        endpoint.set_default_client_config(client_config.clone());
        Ok(Client {
            trusted_certs: rustls::RootCertStore::empty(),
            endpoint,
        })
    }

    pub fn trust_cert(&mut self, cert: CertificateDer<'static>) -> Result<()> {
        // To trust a new cert we need to set a new default client config with the updated cert store
        // This default config is then used for each new connection.
        self.trusted_certs.add(cert)?;

        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(self.trusted_certs.clone())
            .with_no_client_auth();

        let client_config =
            quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));

        self.endpoint
            .set_default_client_config(client_config.clone());

        Ok(())
    }

    pub async fn connect(
        &mut self,
        server_addr: SocketAddr,
        server_name: &str,
    ) -> Result<Connection> {
        let conn = self.endpoint.connect(server_addr, server_name)?.await?;
        Ok(conn)
    }
}
