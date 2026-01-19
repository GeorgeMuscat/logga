#![allow(dead_code)]
use anyhow::Result;
use itertools::Itertools;
use std::{net::Ipv4Addr, path::PathBuf};
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
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
// Originally had this as a generic, however instead maybe it makes more sense to have types to be on a "processor".
// Maybe something like Processor<I, O>
// Maybe we need this on traits, such that we have Source<O> and Sink<I>. This would then have a method such as send and recv.
// Essentially we can think of source and sinks as wrappers around channels.
pub struct FileSource {
    // Going to start with an implementation that should be simple and
    // Need to decide how to handle the case where the file moves, both when we are in the middle or reading or not currently reading.
    // https://docs.rs/inotify/latest/inotify/ can maybe use this to have really efficient reading of files

    // This should only be used for logging or showing where this struct was constructed from in a config module. May also want to have another method for this.
    // Careful using this, as path will not change if the file at the path at the time of initialisation is moved.
    // This means it is possible for `FileSource.path` and `FileSource.file` to be referring to different files.
    name: String,
    path: PathBuf,
    file: File,
    delimiter: u8,
    out_chans: Vec<Sender<Vec<u8>>>,
}

pub struct FileSink {
    name: String,
    path: PathBuf,
    file: File,
    delimiter: u8,
    inp_chan: Receiver<Vec<u8>>,
}

impl FileSource {
    pub async fn new(name: String, path: PathBuf, delimiter: u8) -> Result<Self> {
        Self::new_with_channels(name, path, delimiter, vec![]).await
    }

    pub async fn new_with_channels(
        name: String,
        path: PathBuf,
        delimiter: u8,
        channels: impl IntoIterator<Item = Sender<Vec<u8>>>,
    ) -> Result<Self> {
        // Open this here, because we want to stop
        let mut file = File::open(&path).await?;
        file.set_max_buf_size(16 * 1024);
        Ok(Self {
            name,
            path,
            file,
            delimiter,
            out_chans: channels.into_iter().collect(),
        })
    }

    pub fn register_channel(&mut self, channel: Sender<Vec<u8>>) -> Result<()> {
        self.out_chans.push(channel);
        Ok(())
    }

    pub async fn start(self) -> Result<()> {
        let file = self.file;

        let mut buf = vec![];

        let mut reader = BufReader::with_capacity(4 * 2048, file);
        loop {
            let read_result = reader.read_until(self.delimiter, &mut buf).await;

            while let Some(idx) = buf.iter().position(|b| b == &self.delimiter) {
                // Drop the delimiter, as we don't want to the send it to a receiving module.
                let msg = buf.drain(..=idx).dropping_back(1).collect::<Vec<u8>>();
                for chan in &self.out_chans {
                    chan.send(msg.clone()).await?
                }
            }

            if let Err(err) = read_result {
                // Send what we have and return.
                let msg = buf;
                for chan in &self.out_chans {
                    chan.send(msg.clone()).await?
                }
                return Err(err.into());
            } else {
                let count = read_result.unwrap();
                if count == 0 {
                    return Ok(());
                }
            }
        }
    }
}

impl FileSink {
    pub async fn new(
        name: String,
        path: PathBuf,
        delimiter: u8,
        recv: Receiver<Vec<u8>>,
    ) -> Result<Self> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .await?;
        Ok(Self {
            name,
            path,
            file,
            delimiter,
            inp_chan: recv,
        })
    }

    pub async fn start(mut self) -> Result<()> {
        let mut file = self.file;
        while let Some(mut msg) = self.inp_chan.recv().await {
            // Write each byte and add the delimiter specified
            // TODO: check if this make msg expand an unreasonable amount (both size and freq)
            msg.push(self.delimiter);
            file.write_all(msg.as_ref()).await?;

            // For now, just flush every time.
            file.flush().await?
        }
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
