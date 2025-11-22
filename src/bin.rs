use anyhow::Result;
use loggalib::comms::recv::Server;

mod agent;
mod module;

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    println!("test");

    Ok(())
}
