//! Public API for building a pipeline of modules to process logs.

use std::collections::HashMap;

use crate::{LogSink, LogSource};

pub struct SourceId(usize);
pub struct SinkId(usize);

pub struct PipelineBuilder {
    // Need a way to track the sources, sinks and connections prior to when we call build.
    source_map: HashMap<SourceId, Box<dyn LogSource>>,
    sink_map: HashMap<SinkId, Box<dyn LogSource>>,
    connections: Vec<(SourceId, SinkId)>

};

pub struct Pipeline;
pub struct RunningPipeline;

impl PipelineBuilder {
    // NEXT: implement these here.
    // THEN look at src/lib/module.rs and figure out if we want to change what is pub.
    // THEN implement the old file logic in the new module style, write a test, create a pipeline and pass.
    pub fn add_source(&mut self, source: impl LogSource) {

    }

    pub fn add_sink(&mut self, sink: impl LogSink) {}

    pub fn connect(&mut self, source: SourceId, sink: SinkId) -> Result<(), BuildError> {}
}
