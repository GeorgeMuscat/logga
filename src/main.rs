use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use crate::receive::Server;
mod receive;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let server = Server::new(None)?;
    // Create the server, then TODO: we make a tonne of clients to connect to the server.
    server
        .serve(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7777))
        .await;

    Ok(())
}
