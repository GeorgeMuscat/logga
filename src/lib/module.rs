#![allow(dead_code)]

use std::net::Ipv4Addr;

/// Generic module contract hooking processing nodes together.
pub trait Module<I, O> {
    fn read(&self, inp: I) -> O;
}

/// Marker trait for source nodes that emit data into the pipeline.
pub trait Source {}

/// Marker trait for sinks that consume data from the pipeline.
pub trait Sink {}

/// Placeholder for an eventual QUIC-based source implementation.
pub struct QUICSource {
    pub listen_addr: Ipv4Addr,
    pub listen_port: u16,
}

/// Placeholder for an eventual QUIC-based sink implementation.
pub struct QUICSink {
    pub peer_addr: Ipv4Addr,
    pub peer_port: u16,
}
