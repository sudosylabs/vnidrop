use super::*;
use serde_json::json;

struct NoopSink;

impl CoreEventSink for NoopSink {
    fn on_event(&self, _: CoreEvent) {}
}

#[tokio::test]
async fn a_full_progress_queue_preserves_lifecycle_and_error_events() {
    let temp = tempfile::tempdir().unwrap();
    let repository = Repository::open(temp.path()).await.unwrap();
    let hub = EventHub::start(repository.clone(), Arc::new(NoopSink), 1, 20);

    // The current-thread runtime cannot poll persistence until this task yields.
    hub.emit_transfer(7, "receive", "export", "progress", json!({"exported": 1}));
    hub.emit_transfer(7, "receive", "export", "progress", json!({"exported": 2}));
    hub.emit_transfer(7, "receive", "lifecycle", "done", json!({}));
    hub.emit_transfer(8, "receive", "error", "failed", json!({}));
    hub.emit_endpoint("targeted_transfer", "completed", json!({}));
    hub.flush().await;

    let events = repository.list_events(None, 20).await.unwrap();
    let kinds = events
        .iter()
        .map(|event| event.kind.as_str())
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["completed", "failed", "done", "progress"]);
    hub.shutdown().await;
}
