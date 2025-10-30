use crate::send::Client;
use anyhow::Result;
use core::str;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

mod receive;
mod send;

/// Will have issues if `n_clients` and `n_servers` are too big (i.e. n_clients + n_servers > available ports)
/// Also will probably have issues for other reasons (e.g. logging to stdout) before you reach those numbers...
/// This function does not do things perfectly at all, but this is hopefully to make it easier to understand.
async fn many_to_many(n_clients: u16, n_servers: u16) -> Result<()> {
    // Want to create many servers and have many clients connect to many servers.
    const BASE_SRV_PORT: u16 = 6666;
    let mut certs = vec![];
    let mut clients = vec![];
    for i in 0..n_servers {
        // Create servers, record their certs to import to clients and start the servers.
        let srv = receive::Server::new(None).unwrap();
        certs.push(srv.get_cert().clone());
        tokio::spawn(async move {
            srv.serve(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                BASE_SRV_PORT + i,
            ))
            .await?;
            Ok::<(), anyhow::Error>(())
        });
    }

    for _ in 0..n_clients {
        // TIL: setting port to 0 just picks any free port
        clients.push(Client::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).unwrap());
    }

    for client in clients.iter_mut() {
        for srv_idx in 0..n_servers {
            // Register cert w client and then connect
            client.trust_cert(certs[srv_idx as usize].clone())?;
            client
                .connect(
                    SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::LOCALHOST),
                        BASE_SRV_PORT + (srv_idx as u16),
                    ),
                    "localhost",
                )
                .await?;
        }
    }

    let mut handles = vec![];

    for (client_idx, client) in clients.into_iter().enumerate() {
        handles.push(tokio::spawn(async move {
            for conn in client.connections.iter() {
                // Open bi, send hello and wait until we get a response and log it out.
                let (mut send, mut recv) = conn.open_bi().await?;

                send.write_all(format!("Hello from client {}", client_idx).as_bytes())
                    .await?;
                // Let the recipient know that we are not sending any more data over this stream.
                send.finish()?;

                // We  don't need this stopped call because the server will not send a reply message until it has finished reading our "Hello" message.
                // Keep this in for demo's sake
                send.stopped().await?;

                // Don't need to wait to receive the finish() because that is handled by the read_to_end() command.
                let resp = recv.read_to_end(usize::MAX).await?;

                // We don't escape the content sent from the server because this is a demo/poc
                println!(
                    "client {} reporting message: {}",
                    client_idx.to_string(),
                    str::from_utf8(&resp)?
                );

                // Can safely close these stream(s) (which automatically happens when we drop the stream(s), but we are doing it explicitly for sake of the demo)
                // see below for more detailed explanation.
                conn.close(0u32.into(), b"done");

                // Assume that we are only waiting for one message and the connection can be closed. This is an example of an application protocol determining when we are done.
                // In HTTP3, it seems that each request creates a new bi-directional stream on the existing connection, so we could do that too.
            }
            Ok::<(), anyhow::Error>(())
        }));
    }
    // In order to reliably close the connection, we need to have it such that the final message and all prior messages being sent (regardless of direction) are successfully read in to the application.
    // Whether this is the traditional "client" (the initiator of the connection) or "server" (the listener) depends on the application protocol.
    // In this demo protocol, we expect the final message to be sent from the "server" (thus, the sender) and received by the "client" (thus, the receiver).
    //
    // This means it is on the "client" to terminate the connection once it is sure that all of the reply message has been received.
    // We know the message has been received properly when we the server calls `send.finish()`. As each connection only has a single stream, we know that we can kill the connection.
    // e.g. `conn.close(0u32.into(), b"done");`
    //
    // https://www.iroh.computer/blog/closing-a-quic-connection
    //
    // I think it is good practice to have the peer that is the last to read from their respective stream to be the one to close the connection (assuming that both peers are confident that no more streams will be opened).
    // This means that if we have a bi-directional stream between peers A and B (A <-> B). It is also important to remember that even though a peer has finished writing to a stream (e.g. A->B), we cannot be sure that the other peer has fully read from the stream in to the application.
    // The stream can only be considered "fully used" once both peers can be certain that that stream has been BOTH:
    //     i)  Finished being written to
    //     ii) Finished being read from
    //
    // For example, assuming that only one stream will be used per connection, if A informs B that it has finished writing to the stream A->B and then B informs A that it has finished reading from the A->B stream (either through an explicit message or through application behaviour), but B has not informed A that is finished with the stream both peers should maintain the connection.
    //
    // Once B has informed A that is no longer writing to the B->A stream, and A has finished reading from the buffer, A knows both streams are done with and knows it is safe to close the connection.
    // It is important to note that B cannot close the connection even though it is aware that the A->B stream has been finished with and the B->A stream has had all the data written to it, because the B cannot be sure that A has finished reading from it.

    for handle in handles {
        handle.await??;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    many_to_many(10, 10).await
}
