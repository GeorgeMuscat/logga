use crate::send::Client;
use anyhow::Result;
use core::str;
use std::{
    io::{Write},
    net::{IpAddr, Ipv4Addr, SocketAddr},
};
use tracing::{Level, info, span};

mod receive;
mod send;

async fn many_to_many() -> Result<()> {
    // Want to create many servers and have many clients connect to many servers.
    let span = span!(Level::TRACE, "many_to_many");
    let _enter = span.enter();
    info!("SPAN");
    let base_srv_port = 6666;
    let mut certs = vec![];
    let mut clients = vec![];
    for i in 0..3 {
        // Create a server and client for each loop

        let srv = receive::Server::new(None).unwrap();

        certs.push(srv.get_cert().clone());
        tokio::spawn(async move {
            srv.serve(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                base_srv_port + i,
            ))
            .await?;
            Ok::<(), anyhow::Error>(())
        });
        clients.push(Client::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).unwrap());
    }

    for i in 0..3 {
        for srv_idx in 0..3 {
            // Register cert w client and then connect
            clients[i].trust_cert(certs[srv_idx].clone())?;
            clients[i]
                .connect(
                    SocketAddr::new(
                        IpAddr::V4(Ipv4Addr::LOCALHOST),
                        base_srv_port + (srv_idx as u16),
                    ),
                    "localhost",
                )
                .await?;
        }
    }

    info!("registered certs");

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

                // We likely don't even need this stopped call because the application will wait until we get all the data from the server.
                // Due to how we have designed the application, this will only happen
                let stopped_fut = send.stopped();

                let resp = recv.read_to_end(usize::MAX).await?;

                // We don't escape the content sent from the server because this is a demo/poc
                println!(
                    "client {} reporting message: {}",
                    client_idx.to_string(),
                    str::from_utf8(&resp)?
                );

                stopped_fut.await?;

                // Don't need to wait to receive the finish() because that is handled by the read_to_end() command.

                // Can safely close these stream(s) (which automatically happens when we drop the stream(s), but we are doing it explicitly just cause)
                // see below for more detailed explanation.
                conn.close(0u32.into(), b"done");

                // Don't call Endpoint::wait_idle() until ALL connections have finished.

                // Assume that we are only waiting for one message and the connection can be closed. This is an example of an application protocol determining when we are done.
                // In HTTP3, it seems that each request creates a new bi-directional stream on the existing connection, so we could do that too.
            }
            Ok::<(), anyhow::Error>(())
        }));
    }
    // In order to reliably close the connection, we need to have it such that the final message and all prior messages being sent (regardless of direction) are successfully read in to the application.
    // Whether this is the traditional "client" (the initiator of the connection) or "server" (the listener) depends on the application protocol.
    // In this "protocol", we expect the final message to be sent from the "server" (thus, the sender) and received by the "client" (thus, the receiver).
    //
    // This means it is on the "client" to terminate the connection once it is sure that all of the reply message has been received.
    // We know the message has been received properly when we the server calls `send.finish()`. As each connection only has a single stream, we know that we can kill the connection.
    // e.g. `conn.close(0u32.into(), b"done");`
    //
    // https://www.iroh.computer/blog/closing-a-quic-connection
    //
    //  it is good practice to have the `receiver` (e.g. server) close the connection if there are no more open streams (the sender must still inform the recipient that the specific stream will have no more data sent over it).
    // This requires that the application protocol that we use on top of QUIC (e.g. HTTP) to define when the connection should be closed.
    // As this is just an example, we can consider in our application procotol, that a single response from the server is the final data we expect to receive.

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
    many_to_many().await
}
