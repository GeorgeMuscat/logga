use loggalib::{
    LogSink, LogSource, ModuleFuture, SinkContext, SourceContext,
    pipeline::{BuildError, PipelineBuilder},
};

struct TestSource;

impl LogSource for TestSource {
    fn run(self: Box<Self>, _context: SourceContext) -> ModuleFuture {
        panic!("connection tests must not run modules")
    }
}

struct TestSink;

impl LogSink for TestSink {
    fn run(self: Box<Self>, _context: SinkContext) -> ModuleFuture {
        panic!("connection tests must not run modules")
    }
}

#[test]
fn registers_boxed_source_alongside_concrete_sources() {
    let mut builder = PipelineBuilder::new();
    let sink = builder.add_sink(TestSink);
    let first_source = builder.add_source(TestSource);
    let boxed_source: Box<dyn LogSource> = Box::new(TestSource);
    let second_source = builder.add_boxed_source(boxed_source);
    let third_source = builder.add_source(TestSource);

    assert_ne!(first_source, second_source);
    assert_ne!(second_source, third_source);
    assert_ne!(first_source, third_source);
    for source in [first_source, second_source, third_source] {
        builder.connect(source, sink).unwrap();
    }
}

#[test]
fn registers_boxed_sink_alongside_concrete_sinks() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);
    let first_sink = builder.add_sink(TestSink);
    let boxed_sink: Box<dyn LogSink> = Box::new(TestSink);
    let second_sink = builder.add_boxed_sink(boxed_sink);
    let third_sink = builder.add_sink(TestSink);

    assert_ne!(first_sink, second_sink);
    assert_ne!(second_sink, third_sink);
    assert_ne!(first_sink, third_sink);
    for sink in [first_sink, second_sink, third_sink] {
        builder.connect(source, sink).unwrap();
    }
}

#[test]
fn rejects_unknown_source_without_recording_connection() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);
    let sink = builder.add_sink(TestSink);
    builder.connect(source, sink).unwrap();

    // Obtain an opaque ID beyond this builder's registered sources.
    let mut other_builder = PipelineBuilder::new();
    other_builder.add_source(TestSource);
    let unknown_source = other_builder.add_source(TestSource);

    assert!(matches!(
        builder.connect(unknown_source, sink),
        Err(BuildError::UnknownSource(id)) if id == unknown_source
    ));

    // Once that index is registered, the rejected edge must still be available.
    let next_source = builder.add_source(TestSource);
    assert_eq!(next_source, unknown_source);
    assert!(builder.connect(next_source, sink).is_ok());
    assert!(matches!(
        builder.connect(source, sink),
        Err(BuildError::DuplicateConnection { source: s, sink: t })
            if s == source && t == sink
    ));
}

#[test]
fn rejects_unknown_sink_without_recording_connection() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);
    let sink = builder.add_sink(TestSink);
    builder.connect(source, sink).unwrap();

    // Obtain an opaque ID beyond this builder's registered sinks.
    let mut other_builder = PipelineBuilder::new();
    other_builder.add_sink(TestSink);
    let unknown_sink = other_builder.add_sink(TestSink);

    assert!(matches!(
        builder.connect(source, unknown_sink),
        Err(BuildError::UnknownSink(id)) if id == unknown_sink
    ));

    // Once that index is registered, the rejected edge must still be available.
    let next_sink = builder.add_sink(TestSink);
    assert_eq!(next_sink, unknown_sink);
    assert!(builder.connect(source, next_sink).is_ok());
    assert!(matches!(
        builder.connect(source, sink),
        Err(BuildError::DuplicateConnection { source: s, sink: t })
            if s == source && t == sink
    ));
}

#[test]
fn build_rejects_source_with_no_sinks() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);

    assert!(matches!(
        builder.build(),
        Err(BuildError::UnconnectedSource(id)) if id == source
    ));
}

#[test]
fn build_rejects_sink_with_no_sources() {
    let mut builder = PipelineBuilder::new();
    let sink = builder.add_sink(TestSink);

    assert!(matches!(
        builder.build(),
        Err(BuildError::UnconnectedSink(id)) if id == sink
    ));
}

#[test]
fn build_rejects_unconnected_source_alongside_connected_nodes() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);
    let unconnected_source = builder.add_source(TestSource);
    let sink = builder.add_sink(TestSink);
    builder.connect(source, sink).unwrap();

    assert!(matches!(
        builder.build(),
        Err(BuildError::UnconnectedSource(id)) if id == unconnected_source
    ));
}

#[test]
fn build_rejects_unconnected_sink_alongside_connected_nodes() {
    let mut builder = PipelineBuilder::new();
    let source = builder.add_source(TestSource);
    let sink = builder.add_sink(TestSink);
    let unconnected_sink = builder.add_sink(TestSink);
    builder.connect(source, sink).unwrap();

    assert!(matches!(
        builder.build(),
        Err(BuildError::UnconnectedSink(id)) if id == unconnected_sink
    ));
}
