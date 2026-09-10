#![allow(dead_code)]
use anyhow::Result;

use tokio::sync::mpsc::Sender;

/// Generic module contract hooking processing nodes together.
pub trait Module<T> {
    fn register_channel(&mut self, channel: Sender<T>) -> Result<()>;
}

/// Marker trait for source nodes that emit data into the pipeline.
pub trait Source<T: Clone + Send> {
    async fn send(&mut self, msg: T) -> Result<()>;
}

/// Marker trait for sinks that consume data from the pipeline.
pub trait Sink<T> {
    fn recv(&mut self) -> Result<T>;
}
