//! Public API for building a pipeline of modules to process logs.

use derive_more::Display;
use std::collections::HashSet;
use tokio::io::Sink;

use crate::{LogSink, LogSource};

#[derive(Eq, PartialEq, Clone, Copy, Hash, Debug, Display)]
pub struct SourceId(usize);

#[derive(Eq, PartialEq, Clone, Copy, Hash, Debug, Display)]
pub struct SinkId(usize);

#[derive(Debug)]
pub enum BuildError {
    UnknownSource(SourceId),
    UnknownSink(SinkId),
    DuplicateConnection { source: SourceId, sink: SinkId },
    UnconnectedSource(SourceId),
    UnconnectedSink(SinkId),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSource(id) => {
                write!(f, "unknown source: {id:?}")
            }
            Self::UnknownSink(id) => {
                write!(f, "unknown sink: {id:?}")
            }
            Self::DuplicateConnection { source, sink } => {
                write!(f, "duplicate connection: {source:?} -> {sink:?}")
            }
            Self::UnconnectedSource(id) => {
                write!(f, "unconnected source: {id:?}")
            }
            Self::UnconnectedSink(id) => {
                write!(f, "unconnected sink: {id:?}")
            }
        }
    }
}

impl std::error::Error for BuildError {}
pub struct PipelineBuilder {
    // Need a way to track the sources, sinks and connections prior to when we call build.
    // TODO: consider using something like a vec, as ids are currently just usize and we can use that as index.
    sources: Vec<Box<dyn LogSource>>,
    sinks: Vec<Box<dyn LogSink>>,
    connections: HashSet<(SourceId, SinkId)>,
}

// `Pipeline` needs to hold
// - A reference to each module, so that the pipeline can be started
//
// `Pipeline` should only be obtainable by calling `build` on a `PipelineBuilder`
pub struct Pipeline {
    sources: Vec<Box<dyn LogSource>>,
    sinks: Vec<Box<dyn LogSink>>,
}
pub struct RunningPipeline;

impl PipelineBuilder {
    pub fn new() -> PipelineBuilder {
        Self {
            sources: vec![],
            sinks: vec![],
            connections: HashSet::new(),
        }
    }

    // NEXT: implement these here.
    // THEN look at src/lib/module.rs and figure out if we want to change what is pub.
    // THEN implement the old file logic in the new module style, write a test, create a pipeline and pass.
    pub fn add_source(&mut self, source: impl LogSource) -> SourceId {
        self.add_boxed_source(Box::new(source))
    }

    pub fn add_sink(&mut self, sink: impl LogSink) -> SinkId {
        self.add_boxed_sink(Box::new(sink))
    }

    pub fn add_boxed_source(&mut self, source: Box<dyn LogSource>) -> SourceId {
        self.sources.push(source);
        SourceId(self.sources.len() - 1)
    }

    pub fn add_boxed_sink(&mut self, sink: Box<dyn LogSink>) -> SinkId {
        self.sinks.push(sink);
        SinkId(self.sinks.len() - 1)
    }

    pub fn connect(&mut self, source: SourceId, sink: SinkId) -> Result<(), BuildError> {
        // TODO: Consider doing more validation here?
        if self.connections.contains(&(source, sink)) {
            return Err(BuildError::DuplicateConnection { source, sink });
        }

        if source.0 >= self.sources.len() {
            return Err(BuildError::UnknownSource(source));
        }

        if sink.0 >= self.sinks.len() {
            return Err(BuildError::UnknownSink(sink));
        }

        self.connections.insert((source, sink));
        Ok(())
    }

    pub fn build(self) -> Result<Pipeline, BuildError> {
        // TODO: impl
        // validate and set up a structure of modules that are sending to each other

        // we know that we cannot have any duplicate connections or invalid source ids in connections at this stage.
        // There could still be sources or sinks that are not connected, need to weed those out first.

        {
            let mut connected_sources = vec![false; self.sources.len()];
            let mut connected_sinks = vec![false; self.sinks.len()];

            for (source, sink) in self.connections {
                connected_sources[source.0] = true;
                connected_sinks[sink.0] = true;
            }

            if let Some(idx) = connected_sources.iter().position(|x| x == &false) {
                return Err(BuildError::UnconnectedSource(SourceId(idx)));
            }

            if let Some(idx) = connected_sinks.iter().position(|x| x == &false) {
                return Err(BuildError::UnconnectedSink(SinkId(idx)));
            }

            // All sources and sinks appear as a connection at least once
        }

        // Start connecting the pipes!
        // Each sink will need to store a consumer side of the mpsc.
        // TODO
        todo!()
    }
}

impl Default for PipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}
