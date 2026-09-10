use std::net::SocketAddr;

use anyhow::{Result, anyhow};
use quinn::Endpoint;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    task::JoinHandle,
};

// Instead of using  crate::comms::send::Client; we can just hand out clones of a single Endpoint.
// This would prevent needed a shared borrow of the Client and the accompanying lifetime management (which has the same issue of not being able to be modified)
// Downside is that cannot change things like trusted certs once the executable is running.

/// Placeholder for an eventual QUIC-based source implementation.
pub struct QUICSource {
    endpoint: Endpoint,
    out_chans: Vec<Sender<Vec<u8>>>,
}

/// Placeholder for an eventual QUIC-based sink implementation.
pub struct QUICSink {
    endpoint: Endpoint,
    peer_name: String,
    peer_socket: SocketAddr,
    inp_chan: Receiver<Vec<u8>>,
}

impl QUICSink {
    pub async fn new(
        endpoint: Endpoint,
        peer_name: String,
        peer_socket: SocketAddr,
        inp_chan: Receiver<Vec<u8>>,
    ) -> Result<Self> {
        Ok(Self {
            endpoint,
            peer_name,
            peer_socket,
            inp_chan,
        })
    }

    pub async fn start(mut self) -> Result<()> {
        // TODO: loop with exponential backoff to recover if client is not responding for some recoverable reason
        let conn = self
            .endpoint
            .connect(self.peer_socket, &self.peer_name)?
            .await?;

        while let Some(msg) = self.inp_chan.recv().await {
            // We actually want to create this in the loop.
            // it is quick and easy to create a stream.
            // prevents Head-of-Line blocking
            let mut send = conn.open_uni().await?;
            send.write_all(&msg).await?;
            send.finish()?;
            send.stopped().await?;
        }
        Ok(())
    }
}

impl QUICSource {
    pub async fn start(self) -> Result<()> {
        // TODO: decide a more sensible buffer size
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1024);

        // Maybe need mpsc channel to a thread that is responsible for sending the data
        let main_handle: JoinHandle<Result<()>> = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                for chan in &self.out_chans {
                    chan.send(msg.clone()).await?;
                }
            }
            Ok(())
        });
        while let Some(conn) = self.endpoint.accept().await {
            // TODO: decide if we need the output of these tasks.
            let txx = tx.clone();
            tokio::spawn(async move {
                let connection = conn
                    .await
                    .map_err(|e| anyhow!("failed to connect: {}", e))?;

                let mut recv = connection
                    .accept_uni()
                    .await
                    .map_err(|e| anyhow!("failed to open stream: {}", e))?;

                // Recv first, then send data
                let msg = recv
                    .read_to_end(usize::MAX)
                    .await
                    .map_err(|e| anyhow!("failed to read request: {}", e))?;
                txx.send(msg).await?;
                // Make sure we wait for the client to receive all the info
                recv.stop(0u32.into())?;

                Ok::<(), anyhow::Error>(())
            });
        }
        self.endpoint.wait_idle().await;
        main_handle.await??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use quinn::Endpoint;
    use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
    use rcgen::{
        BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
        Issuer, KeyPair, KeyUsagePurpose,
    };
    use rustls::RootCertStore;
    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;
    use tokio::{sync::mpsc, task::JoinHandle};

    fn create_ca() -> Result<(Certificate, KeyPair)> {
        let key_pair = KeyPair::generate()?;

        let mut params = CertificateParams::default();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        params
            .distinguished_name
            .push(DnType::CommonName, "Test CA");
        let cert = params.self_signed(&key_pair)?;
        Ok((cert, key_pair))
    }

    fn issue_leaf_cert(
        ca_cert: &Certificate,
        ca_key_pair: &KeyPair,
        common_name: &str,
    ) -> Result<(Certificate, KeyPair)> {
        let mut params = CertificateParams::new(vec![common_name.to_string()])?;
        params.is_ca = IsCa::NoCa;

        params
            .distinguished_name
            .push(DnType::CommonName, common_name);

        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];

        let key_pair = KeyPair::generate()?;

        let cert = params.signed_by(
            &key_pair,
            &Issuer::from_ca_cert_der(ca_cert.der(), &ca_key_pair)?,
        )?;

        Ok((cert, key_pair))
    }

    fn create_peer_endpoint(
        bind_addr: SocketAddr,
        cert_store: Arc<RootCertStore>,
        cert: &Certificate,
        key_pair: &KeyPair,
    ) -> Result<Endpoint> {
        let key_der = key_pair.serialize_der();

        let server_crypto = rustls::ServerConfig::builder()
            .with_client_cert_verifier(
                rustls::server::WebPkiClientVerifier::builder(cert_store.clone()).build()?,
            )
            .with_single_cert(
                vec![cert.der().clone()],
                PrivatePkcs8KeyDer::from(key_der.clone()).into(),
            )?;
        let server_config =
            quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));
        let mut endpoint = Endpoint::server(server_config, bind_addr)?;

        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(cert_store.as_ref().clone())
            .with_client_auth_cert(
                vec![cert.der().clone()],
                PrivatePkcs8KeyDer::from(key_der).into(),
            )?;
        let client_config =
            quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));
        endpoint.set_default_client_config(client_config);

        Ok(endpoint)
    }

    fn create_test_peer_endpoints() -> Result<(Endpoint, Endpoint)> {
        let mut cert_store = RootCertStore::empty();
        let (ca_cert, ca_key_pair) = create_ca()?;
        cert_store.add(ca_cert.der().clone())?;
        let cert_store = Arc::new(cert_store);

        let (cert_a, key_a) = issue_leaf_cert(&ca_cert, &ca_key_pair, "service-a")?;
        let (cert_b, key_b) = issue_leaf_cert(&ca_cert, &ca_key_pair, "service-b")?;
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);

        Ok((
            create_peer_endpoint(bind_addr, cert_store.clone(), &cert_a, &key_a)?,
            create_peer_endpoint(bind_addr, cert_store, &cert_b, &key_b)?,
        ))
    }

    #[allow(dead_code)]
    fn generate_self_signed_cert() -> Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)>
    {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let cert_der = CertificateDer::from(cert.cert);
        let key = PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der());
        Ok((cert_der, key))
    }

    async fn spawn_quic_sink(
        endpoint: Endpoint,
        peer_name: String,
        peer_addr: SocketAddr,
        rx: Receiver<Vec<u8>>,
    ) -> Result<JoinHandle<Result<()>>> {
        let sink = QUICSink::new(endpoint, peer_name, peer_addr, rx).await?;
        Ok(tokio::spawn(sink.start()))
    }

    async fn spawn_quic_source(
        endpoint: Endpoint,
        out_tx: Sender<Vec<u8>>,
    ) -> Result<JoinHandle<Result<()>>> {
        let source = QUICSource {
            endpoint,
            out_chans: vec![out_tx],
        };
        Ok(tokio::spawn(source.start()))
    }

    async fn receive_message(
        direction: &str,
        receiver: &mut Receiver<Vec<u8>>,
        source_handle: &mut JoinHandle<Result<()>>,
        sink_handle: &mut JoinHandle<Result<()>>,
    ) -> Result<Vec<u8>> {
        tokio::select! {
            result = &mut *sink_handle => {
                match result {
                    Ok(Ok(())) => anyhow::bail!("{direction}: QUICSink exited before delivering the message"),
                    Ok(Err(err)) => anyhow::bail!("{direction}: QUICSink failed: {err:#}"),
                    Err(err) => anyhow::bail!("{direction}: QUICSink task panicked or was cancelled: {err}"),
                }
            }
            result = &mut *source_handle => {
                match result {
                    Ok(Ok(())) => anyhow::bail!("{direction}: QUICSource exited before receiving the message"),
                    Ok(Err(err)) => anyhow::bail!("{direction}: QUICSource failed: {err:#}"),
                    Err(err) => anyhow::bail!("{direction}: QUICSource task panicked or was cancelled: {err}"),
                }
            }
            message = receiver.recv() => {
                message.ok_or_else(|| anyhow!("{direction}: QUIC output channel closed without a message"))
            }
            _ = tokio::time::sleep(Duration::from_secs(3)) => {
                anyhow::bail!("{direction}: timed out waiting for the QUIC round trip")
            }
        }
    }

    #[tokio::test]
    async fn quic_mtls_connection() -> Result<()> {
        // this is allowed to error, as it will error if the provider was initialised by another test case
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let mut cert_store = RootCertStore::empty();
        let (ca_cert, ca_key_pair) = create_ca()?;
        cert_store.add(ca_cert.der().clone())?;
        let cert_store = Arc::new(cert_store);

        let (server_cert, server_key) = issue_leaf_cert(&ca_cert, &ca_key_pair, "service-a")?;
        let (client_cert, client_key) = issue_leaf_cert(&ca_cert, &ca_key_pair, "service-b")?;

        let server_crypto = rustls::ServerConfig::builder()
            .with_client_cert_verifier(
                rustls::server::WebPkiClientVerifier::builder(cert_store.clone()).build()?,
            )
            .with_single_cert(vec![server_cert.der().clone()], server_key.into())?;
        let server_config =
            quinn::ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(server_crypto)?));
        let server_endpoint = Endpoint::server(
            server_config,
            // can use 0 to get an ephemeral port
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        )?;
        let server_addr = server_endpoint.local_addr()?;

        let client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(cert_store.as_ref().clone())
            .with_client_auth_cert(vec![client_cert.der().clone()], client_key.into())?;
        let client_config =
            quinn::ClientConfig::new(Arc::new(QuicClientConfig::try_from(client_crypto)?));
        let mut client_endpoint =
            Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
        client_endpoint.set_default_client_config(client_config);

        let accept = async {
            let incoming = server_endpoint
                .accept()
                .await
                .ok_or_else(|| anyhow!("server endpoint closed before accepting a connection"))?;
            Ok::<_, anyhow::Error>(incoming.await?)
        };
        let connect = async {
            let connecting = client_endpoint.connect(server_addr, "service-a")?;
            Ok::<_, anyhow::Error>(connecting.await?)
        };

        let (server_connection, client_connection) = timeout(Duration::from_secs(3), async {
            tokio::try_join!(accept, connect)
        })
        .await
        .map_err(|_| anyhow!("mTLS handshake timed out"))??;

        assert!(
            server_connection.peer_identity().is_some(),
            "server did not receive the client's TLS identity"
        );
        assert!(
            client_connection.peer_identity().is_some(),
            "client did not receive the server's TLS identity"
        );

        client_connection.close(0u32.into(), b"test complete");
        server_connection.close(0u32.into(), b"test complete");
        Ok(())
    }

    #[tokio::test]
    async fn quic_round_trip_basic() -> Result<()> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let (endpoint_a, endpoint_b) = create_test_peer_endpoints()?;
        let endpoint_a_addr = endpoint_a.local_addr()?;
        let payload = b"hello over quic\n".to_vec();

        let (to_quic_tx, to_quic_rx) = mpsc::channel::<Vec<u8>>(32);
        let (from_quic_tx, mut from_quic_rx) = mpsc::channel::<Vec<u8>>(32);

        let mut source_handle = spawn_quic_source(endpoint_a, from_quic_tx).await?;
        let mut sink_handle = spawn_quic_sink(
            endpoint_b,
            "service-a".to_string(),
            endpoint_a_addr,
            to_quic_rx,
        )
        .await?;

        to_quic_tx.send(payload.clone()).await?;
        let received = receive_message(
            "service-b to service-a",
            &mut from_quic_rx,
            &mut source_handle,
            &mut sink_handle,
        )
        .await?;

        drop(to_quic_tx);
        drop(from_quic_rx);
        sink_handle.abort();
        source_handle.abort();

        assert_eq!(received, payload);
        Ok(())
    }

    #[tokio::test]
    async fn quic_bidirectional_round_trip() -> Result<()> {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let (endpoint_a, endpoint_b) = create_test_peer_endpoints()?;
        let endpoint_a_addr = endpoint_a.local_addr()?;
        let endpoint_b_addr = endpoint_b.local_addr()?;

        let payload_from_a = b"hello from service-a".to_vec();
        let payload_from_b = b"hello from service-b".to_vec();

        let (to_b_tx, to_b_rx) = mpsc::channel::<Vec<u8>>(32);
        let (received_by_b_tx, mut received_by_b_rx) = mpsc::channel::<Vec<u8>>(32);
        let (to_a_tx, to_a_rx) = mpsc::channel::<Vec<u8>>(32);
        let (received_by_a_tx, mut received_by_a_rx) = mpsc::channel::<Vec<u8>>(32);

        let mut source_a_handle = spawn_quic_source(endpoint_a.clone(), received_by_a_tx).await?;
        let mut source_b_handle = spawn_quic_source(endpoint_b.clone(), received_by_b_tx).await?;
        let mut sink_a_handle = spawn_quic_sink(
            endpoint_a,
            "service-b".to_string(),
            endpoint_b_addr,
            to_b_rx,
        )
        .await?;
        let mut sink_b_handle = spawn_quic_sink(
            endpoint_b,
            "service-a".to_string(),
            endpoint_a_addr,
            to_a_rx,
        )
        .await?;

        to_b_tx.send(payload_from_a.clone()).await?;
        to_a_tx.send(payload_from_b.clone()).await?;

        let (received_by_b, received_by_a) = tokio::try_join!(
            receive_message(
                "service-a to service-b",
                &mut received_by_b_rx,
                &mut source_b_handle,
                &mut sink_a_handle,
            ),
            receive_message(
                "service-b to service-a",
                &mut received_by_a_rx,
                &mut source_a_handle,
                &mut sink_b_handle,
            ),
        )?;

        drop(to_a_tx);
        drop(to_b_tx);
        drop(received_by_a_rx);
        drop(received_by_b_rx);
        sink_a_handle.abort();
        sink_b_handle.abort();
        source_a_handle.abort();
        source_b_handle.abort();

        assert_eq!(received_by_b, payload_from_a);
        assert_eq!(received_by_a, payload_from_b);
        Ok(())
    }
}
