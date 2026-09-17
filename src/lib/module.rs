//! Public extension API for custom log sources and sinks.

use std::any::type_name;

use anyhow::{Result, anyhow};
use bytes::Bytes;
use futures::future::BoxFuture;
use tokio::{sync::mpsc, task::JoinHandle};

/// The result type returned by source and sink modules.
pub type ModuleResult<T = ()> = Result<T>;

/// An owned, sendable future returned by a module.
pub type ModuleFuture = BoxFuture<'static, ModuleResult<()>>;

/// A single message travelling through a logga pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogMessage {
    payload: Bytes,
}

impl LogMessage {
    /// Creates a message from an owned or static byte representation.
    pub fn new(payload: impl Into<Bytes>) -> Self {
        Self {
            payload: payload.into(),
        }
    }

    /// Returns the message payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the message and returns its payload.
    pub fn into_payload(self) -> Bytes {
        self.payload
    }
}

impl From<Bytes> for LogMessage {
    fn from(payload: Bytes) -> Self {
        Self::new(payload)
    }
}

impl From<Vec<u8>> for LogMessage {
    fn from(payload: Vec<u8>) -> Self {
        Self::new(payload)
    }
}

impl From<String> for LogMessage {
    fn from(payload: String) -> Self {
        Self::new(payload)
    }
}

/// The sending side of one directed pipeline edge.
#[derive(Clone, Debug)]
pub struct OutputPort {
    sender: mpsc::Sender<LogMessage>,
}

/// The receiving side of one directed pipeline edge.
#[derive(Debug)]
pub struct InputPort {
    receiver: mpsc::Receiver<LogMessage>,
}

/// Creates a bounded pipeline edge.
///
/// Clone an [`OutputPort`] to connect multiple sources to one sink. Give a
/// source multiple output ports to fan its messages out to multiple sinks.
///
/// # Panics
///
/// Panics when `capacity` is zero, matching Tokio's bounded channel behavior.
/// TODO: Consider making this non-pub and instead keep channels fully internal, with only a method on the constructor that allows for the connection of modules.
pub fn channel(capacity: usize) -> (OutputPort, InputPort) {
    let (sender, receiver) = mpsc::channel(capacity);
    (OutputPort { sender }, InputPort { receiver })
}

/// Runtime facilities supplied to a [`LogSource`].
#[derive(Clone, Debug)]
pub struct SourceContext {
    outputs: Vec<OutputPort>,
}

impl SourceContext {
    /// Creates a source context connected to the supplied output ports.
    pub fn new(outputs: impl IntoIterator<Item = OutputPort>) -> Self {
        Self {
            outputs: outputs.into_iter().collect(),
        }
    }

    /// Emits one message to every connected output.
    ///
    /// Sending is sequential, so a slow sink applies backpressure to the
    /// source and therefore to every other output of that source.
    pub async fn emit(&self, message: LogMessage) -> ModuleResult<()> {
        for output in &self.outputs {
            output
                .sender
                .send(message.clone())
                .await
                .map_err(|_| anyhow!("log output closed"))?;
        }
        Ok(())
    }

    /// Returns the number of connected outputs.
    pub fn output_count(&self) -> usize {
        self.outputs.len()
    }

    /// Returns whether this source has no connected outputs.
    pub fn is_disconnected(&self) -> bool {
        self.outputs.is_empty()
    }
}

impl Default for SourceContext {
    fn default() -> Self {
        Self::new([])
    }
}

/// Runtime facilities supplied to a [`LogSink`].
#[derive(Debug)]
pub struct SinkContext {
    input: InputPort,
}

impl SinkContext {
    /// Creates a sink context from one input port.
    pub fn new(input: InputPort) -> Self {
        Self { input }
    }

    /// Waits for the next message, returning `None` after every sender closes.
    pub async fn recv(&mut self) -> Option<LogMessage> {
        self.input.receiver.recv().await
    }
}

/// An externally implementable producer of log messages.
pub trait LogSource: Send + 'static {
    /// A human-readable module name used for diagnostics and supervision.
    fn name(&self) -> &str {
        type_name::<Self>()
    }

    /// Runs this source until completion or failure.
    fn run(self: Box<Self>, context: SourceContext) -> ModuleFuture;
}

/// An externally implementable consumer of log messages.
pub trait LogSink: Send + 'static {
    /// A human-readable module name used for diagnostics and supervision.
    fn name(&self) -> &str {
        type_name::<Self>()
    }

    /// Runs this sink until its input closes or it fails.
    fn run(self: Box<Self>, context: SinkContext) -> ModuleFuture;
}

/// A configured source ready to be spawned.
pub struct SourceNode {
    module: Box<dyn LogSource>,
    context: SourceContext,
}

impl SourceNode {
    /// Creates a node from a concrete custom or built-in source.
    pub fn new(module: impl LogSource, context: SourceContext) -> Self {
        Self::from_boxed(Box::new(module), context)
    }

    /// Creates a node from an already boxed source trait object.
    pub fn from_boxed(module: Box<dyn LogSource>, context: SourceContext) -> Self {
        Self { module, context }
    }

    /// Returns the source's diagnostic name.
    pub fn name(&self) -> &str {
        self.module.name()
    }

    /// Spawns the source on the current Tokio runtime.
    pub fn spawn(self) -> JoinHandle<ModuleResult<()>> {
        tokio::spawn(self.module.run(self.context))
    }
}

/// A configured sink ready to be spawned.
pub struct SinkNode {
    module: Box<dyn LogSink>,
    context: SinkContext,
}

impl SinkNode {
    /// Creates a node from a concrete custom or built-in sink.
    pub fn new(module: impl LogSink, context: SinkContext) -> Self {
        Self::from_boxed(Box::new(module), context)
    }

    /// Creates a node from an already boxed sink trait object.
    pub fn from_boxed(module: Box<dyn LogSink>, context: SinkContext) -> Self {
        Self { module, context }
    }

    /// Returns the sink's diagnostic name.
    pub fn name(&self) -> &str {
        self.module.name()
    }

    /// Spawns the sink on the current Tokio runtime.
    pub fn spawn(self) -> JoinHandle<ModuleResult<()>> {
        tokio::spawn(self.module.run(self.context))
    }
}
