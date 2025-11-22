use std::{fs::File, net::Ipv4Addr, path::PathBuf};

trait Module<I, O> {
    fn read(&self, inp: I) -> O;
}

// Make this module implement whatever trait is "Input" as it provides a stream to be read from
trait Source {}

// Maket his module implement whatever trais is "Output" as it provides a stream to
trait Sink {}

pub struct FileSource {
    // Going to start with an implementation that should be simple and
    // Need to decide how to handle the case where the file moves, both when we are in the middle or reading or not currently reading.
    path: PathBuf,
    fp: File,
}

pub struct FileSink {
    path: PathBuf,
    fp: File,
}

pub struct QUICSource {
    listen_addr: Ipv4Addr,
    listen_port: u16,
}

pub struct QUICSink {
    peer_addr: Ipv4Addr,
    peer_port: u16,
}
