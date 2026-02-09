#![allow(dead_code)]
use anyhow::Result;
use itertools::Itertools;
use std::path::PathBuf;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc::{Receiver, Sender},
};

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
        inp_chan: Receiver<Vec<u8>>,
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
            inp_chan,
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

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use itertools::Itertools;
    use tokio::{fs, sync::mpsc, task::JoinHandle};
    use uuid::Uuid;

    async fn create_temp_file(prefix: &str, contents: &[u8]) -> Result<PathBuf> {
        let path = std::env::temp_dir().join(format!("logga-{prefix}-{}.log", Uuid::new_v4()));
        fs::write(&path, contents).await?;
        Ok(path)
    }

    async fn spawn_sink(
        name: &str,
        delimiter: u8,
        receiver: Receiver<Vec<u8>>,
    ) -> Result<(PathBuf, JoinHandle<Result<()>>)> {
        let path = create_temp_file(&format!("sink-{name}"), b"").await?;
        let sink = FileSink::new(name.to_string(), path.clone(), delimiter, receiver).await?;
        let handle = tokio::spawn(sink.start());
        Ok((path, handle))
    }

    async fn spawn_source(
        name: &str,
        contents: &[u8],
        delimiter: u8,
        channels: Vec<Sender<Vec<u8>>>,
    ) -> Result<(PathBuf, JoinHandle<Result<()>>)> {
        let path = create_temp_file(&format!("source-{name}"), contents).await?;
        let source =
            FileSource::new_with_channels(name.to_string(), path.clone(), delimiter, channels)
                .await?;
        let handle = tokio::spawn(source.start());
        Ok((path, handle))
    }

    async fn cleanup_path(path: &PathBuf) {
        let _ = fs::remove_file(path).await;
    }

    fn sorted_lines(bytes: &[u8]) -> Vec<String> {
        std::str::from_utf8(bytes)
            .unwrap()
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .sorted()
            .collect()
    }

    #[tokio::test]
    async fn file_source_and_sink_round_trip() -> Result<()> {
        let delimiter = b'\n';
        let expected = b"first line\nsecond line\n";

        let (sink_path, sink_handle, input_path, source_handle) = {
            let (sink_tx, sink_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_path, sink_handle) =
                spawn_sink("round-trip-sink", delimiter, sink_rx).await?;
            let (input_path, source_handle) =
                spawn_source("round-trip-source", expected, delimiter, vec![sink_tx]).await?;
            (sink_path, sink_handle, input_path, source_handle)
        };

        source_handle.await??;
        sink_handle.await??;

        let output_bytes = fs::read(&sink_path).await?;
        assert_eq!(output_bytes, expected);

        cleanup_path(&input_path).await;
        cleanup_path(&sink_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn two_sources_one_sink_merge_all_lines() -> Result<()> {
        let delimiter = b'\n';

        let (sink_path, sink_handle, path_a, handle_a, path_b, handle_b) = {
            let (sink_tx, sink_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_path, sink_handle) = spawn_sink("fan-in-sink", delimiter, sink_rx).await?;
            let (path_a, handle_a) = spawn_source(
                "fan-in-source-a",
                b"alpha\nbeta\n",
                delimiter,
                vec![sink_tx.clone()],
            )
            .await?;
            let (path_b, handle_b) = spawn_source(
                "fan-in-source-b",
                b"gamma\ndelta\n",
                delimiter,
                vec![sink_tx],
            )
            .await?;
            (sink_path, sink_handle, path_a, handle_a, path_b, handle_b)
        };

        handle_a.await??;
        handle_b.await??;
        sink_handle.await??;

        let output_bytes = fs::read(&sink_path).await?;
        assert_eq!(
            sorted_lines(&output_bytes),
            vec![
                "alpha".to_string(),
                "beta".to_string(),
                "delta".to_string(),
                "gamma".to_string()
            ]
        );

        cleanup_path(&path_a).await;
        cleanup_path(&path_b).await;
        cleanup_path(&sink_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn one_source_two_sinks_receive_same_data() -> Result<()> {
        let delimiter = b'\n';
        let payload = b"entry-one\nentry-two\n";

        let (
            sink_one_path,
            sink_one_handle,
            sink_two_path,
            sink_two_handle,
            input_path,
            source_handle,
        ) = {
            let (sink_one_tx, sink_one_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_two_tx, sink_two_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_one_path, sink_one_handle) =
                spawn_sink("fan-out-sink-one", delimiter, sink_one_rx).await?;
            let (sink_two_path, sink_two_handle) =
                spawn_sink("fan-out-sink-two", delimiter, sink_two_rx).await?;
            let (input_path, source_handle) = spawn_source(
                "fan-out-source",
                payload,
                delimiter,
                vec![sink_one_tx, sink_two_tx],
            )
            .await?;
            (
                sink_one_path,
                sink_one_handle,
                sink_two_path,
                sink_two_handle,
                input_path,
                source_handle,
            )
        };

        source_handle.await??;

        sink_one_handle.await??;
        sink_two_handle.await??;

        let output_one = fs::read(&sink_one_path).await?;
        let output_two = fs::read(&sink_two_path).await?;
        assert_eq!(output_one, payload);
        assert_eq!(output_two, payload);

        cleanup_path(&input_path).await;
        cleanup_path(&sink_one_path).await;
        cleanup_path(&sink_two_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn two_sources_two_sinks_full_fan_out() -> Result<()> {
        let delimiter = b'\n';

        let (
            sink_one_path,
            sink_one_handle,
            sink_two_path,
            sink_two_handle,
            path_a,
            handle_a,
            path_b,
            handle_b,
        ) = {
            let (sink_one_tx, sink_one_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_two_tx, sink_two_rx) = mpsc::channel::<Vec<u8>>(32);
            let (sink_one_path, sink_one_handle) =
                spawn_sink("matrix-sink-one", delimiter, sink_one_rx).await?;
            let (sink_two_path, sink_two_handle) =
                spawn_sink("matrix-sink-two", delimiter, sink_two_rx).await?;
            let (path_a, handle_a) = spawn_source(
                "matrix-source-a",
                b"a-one\na-two\n",
                delimiter,
                vec![sink_one_tx.clone(), sink_two_tx.clone()],
            )
            .await?;
            let (path_b, handle_b) = spawn_source(
                "matrix-source-b",
                b"b-one\nb-two\n",
                delimiter,
                vec![sink_one_tx, sink_two_tx],
            )
            .await?;
            (
                sink_one_path,
                sink_one_handle,
                sink_two_path,
                sink_two_handle,
                path_a,
                handle_a,
                path_b,
                handle_b,
            )
        };

        handle_a.await??;
        handle_b.await??;

        sink_one_handle.await??;
        sink_two_handle.await??;

        let output_one = fs::read(&sink_one_path).await?;
        let output_two = fs::read(&sink_two_path).await?;
        let expected_lines = vec![
            "a-one".to_string(),
            "a-two".to_string(),
            "b-one".to_string(),
            "b-two".to_string(),
        ];
        let expected_lines_clone = expected_lines.clone();
        assert_eq!(sorted_lines(&output_one), expected_lines);
        assert_eq!(sorted_lines(&output_two), expected_lines_clone);

        cleanup_path(&path_a).await;
        cleanup_path(&path_b).await;
        cleanup_path(&sink_one_path).await;
        cleanup_path(&sink_two_path).await;
        Ok(())
    }
}
