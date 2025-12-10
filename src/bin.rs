use std::{fs::File, path::PathBuf};

use anyhow::Result;
use loggalib::comms::recv::Server;
use tokio::sync::mpsc;

use crate::module::{FileSink, FileSource};

mod agent;
mod module;

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // TODO: do config parsing here once we have it implemented.

    // For now, just do simple file input and output
    // TODO: handle errors better

    // Need to construct the consumers first, so that we can create the required
    // channels to pass to the producers/senders
    //
    // TODO: When configs eventually become a thing, we would essentially construct a graph of flows,
    // and create mpsc channels from "right to left" or "sink to source"
    let (mut send, mut recv) = mpsc::channel::<&[u8]>(100); // TODO: create a sensible default and a method to customise based on Bytes or something
    let output_path = PathBuf::from("./test-out.log");
    let delimiter = "\n".as_bytes();
    let output = FileSink::new("Sink1".to_string(), output_path.clone(), delimiter, recv).await?;

    let input_path = PathBuf::from("./test-in.log");
    dbg!(&input_path);
    let input =
        FileSource::new_with_channels("Source1".to_string(), input_path, delimiter, vec![send])
            .await?;

    // Create threads, one per module for now.
    // TODO: Consider how we handle one of the threads crashing/stopping.
    // TODO: In future there might be smarter ways to do threading.
    tokio::spawn(output.start());
    tokio::spawn(input.start());
    Ok(())
}
