use anyhow::Result;
use std::{net::Ipv4Addr, path::PathBuf};
use tokio::{
    fs::File,
    io::{AsyncBufRead, AsyncReadExt, BufReader},
    sync::mpsc::{Receiver, Sender},
};
trait Module<I, O> {
    fn read(&self, inp: I) -> O;
}

// Make this module implement whatever trait is "Input" as it provides a stream to be read from
trait Source {}

// When implementing processing, might be cool to use `rayon` for things like dedupe or

// Make his module implement whatever trais is "Output" as it provides a stream to
trait Sink {}

pub struct FileSource<'a, T> {
    // Going to start with an implementation that should be simple and
    // Need to decide how to handle the case where the file moves, both when we are in the middle or reading or not currently reading.
    // https://docs.rs/inotify/latest/inotify/ can maybe use this to have really efficient reading of files

    // This should only be used for logging or showing where this struct was constructed from in a config module. May also want to have another method for this.
    // Careful using this, as path will not change if the file at the path at the time of initialisation is moved.
    // This means it is possible for `FileSource.path` and `FileSource.file` to be referring to different files.
    name: String,
    path: PathBuf,
    file: File,
    delimiter: &'a [u8],
    out_chans: Vec<Sender<T>>,
}

pub struct FileSink<'a, T> {
    name: String,
    path: PathBuf,
    file: File,
    delimiter: &'a [u8],
    inp_chan: Receiver<T>,
}

impl<'a, T> FileSource<'a, T> {
    pub async fn new(name: String, path: PathBuf, delimiter: &'a [u8]) -> Result<Self> {
        Self::new_with_channels(name, path, delimiter, vec![]).await
    }

    pub async fn new_with_channels(
        name: String,
        path: PathBuf,
        delimiter: &'a [u8],
        channels: impl IntoIterator<Item = tokio::sync::mpsc::Sender<T>>,
    ) -> Result<Self> {
        // Open this here, because we want to stop
        let file = File::open(&path).await?;
        Ok(Self {
            name,
            path,
            file,
            delimiter,
            out_chans: channels.into_iter().collect(),
        })
    }

    pub fn register_channel(&mut self, channel: Sender<T>) -> Result<()> {
        self.out_chans.push(channel);
        Ok(())
    }

    pub async fn start(self) -> Result<()> {
        let mut reader = BufReader::new(self.file.try_clone().await?);
        loop {
            // Keep reading from the file until EOF.
            // TODO: impl
        }
    }
}

impl<'a, T> FileSink<'a, T> {
    pub async fn new(
        name: String,
        path: PathBuf,
        delimiter: &'a [u8],
        recv: Receiver<T>,
    ) -> Result<Self> {
        let file = File::open(&path).await?;
        Ok(Self {
            name,
            path,
            file,
            delimiter,
            inp_chan: recv,
        })
    }

    pub async fn start(self) -> Result<()> {
        Ok(())
    }
}

pub struct QUICSource {
    listen_addr: Ipv4Addr,
    listen_port: u16,
}

pub struct QUICSink {
    peer_addr: Ipv4Addr,
    peer_port: u16,
}
