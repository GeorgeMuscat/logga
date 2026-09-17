use std::sync::{Arc, Mutex};

use loggalib::{
    LogMessage, LogSink, LogSource, ModuleFuture, SinkContext, SinkNode, SourceContext, SourceNode,
    channel,
};

struct CustomSource {
    messages: Vec<LogMessage>,
}

impl LogSource for CustomSource {
    fn name(&self) -> &str {
        "custom-source"
    }

    fn run(self: Box<Self>, context: SourceContext) -> ModuleFuture {
        Box::pin(async move {
            for message in self.messages {
                context.emit(message).await?;
            }
            Ok(())
        })
    }
}

struct CustomSink {
    messages: Arc<Mutex<Vec<LogMessage>>>,
}

impl LogSink for CustomSink {
    fn name(&self) -> &str {
        "custom-sink"
    }

    fn run(self: Box<Self>, mut context: SinkContext) -> ModuleFuture {
        Box::pin(async move {
            while let Some(message) = context.recv().await {
                self.messages.lock().unwrap().push(message);
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn external_source_and_sink_communicate_through_public_api() {
    let (output, input) = channel(8);
    let received = Arc::new(Mutex::new(Vec::new()));

    let source: Box<dyn LogSource> = Box::new(CustomSource {
        messages: vec![LogMessage::new("first"), LogMessage::new("second")],
    });
    let sink: Box<dyn LogSink> = Box::new(CustomSink {
        messages: received.clone(),
    });

    let source = SourceNode::from_boxed(source, SourceContext::new([output]));
    let sink = SinkNode::from_boxed(sink, SinkContext::new(input));
    assert_eq!(source.name(), "custom-source");
    assert_eq!(sink.name(), "custom-sink");

    let sink_handle = sink.spawn();
    let source_handle = source.spawn();

    source_handle.await.unwrap().unwrap();
    sink_handle.await.unwrap().unwrap();

    let payloads = received
        .lock()
        .unwrap()
        .iter()
        .map(|message| message.payload().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(payloads, vec![b"first".to_vec(), b"second".to_vec()]);
}

#[tokio::test]
async fn source_context_fans_out_to_multiple_custom_sinks() {
    let (first_output, first_input) = channel(8);
    let (second_output, second_input) = channel(8);
    let first_received = Arc::new(Mutex::new(Vec::new()));
    let second_received = Arc::new(Mutex::new(Vec::new()));

    let source = SourceNode::new(
        CustomSource {
            messages: vec![LogMessage::new("shared")],
        },
        SourceContext::new([first_output, second_output]),
    );
    let first_sink = SinkNode::new(
        CustomSink {
            messages: first_received.clone(),
        },
        SinkContext::new(first_input),
    );
    let second_sink = SinkNode::new(
        CustomSink {
            messages: second_received.clone(),
        },
        SinkContext::new(second_input),
    );

    let first_sink_handle = first_sink.spawn();
    let second_sink_handle = second_sink.spawn();
    source.spawn().await.unwrap().unwrap();
    first_sink_handle.await.unwrap().unwrap();
    second_sink_handle.await.unwrap().unwrap();

    assert_eq!(first_received.lock().unwrap()[0].payload(), b"shared");
    assert_eq!(second_received.lock().unwrap()[0].payload(), b"shared");
}
