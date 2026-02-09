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
    name: String,
    endpoint: Endpoint,
    peer_name: String,
    peer_socket: SocketAddr,
    inp_chan: Receiver<Vec<u8>>,
}

impl QUICSink {
    pub async fn new(
        name: String,
        endpoint: Endpoint,
        peer_name: String,
        peer_socket: SocketAddr,
        inp_chan: Receiver<Vec<u8>>,
    ) -> Result<Self> {
        Ok(Self {
            name,
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
            send.write_all(&msg);
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
