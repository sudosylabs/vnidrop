use vnidrop::CoreEvent;

/// Bounded, newest-first history reconciles live callbacks with persisted startup events.
pub struct EventHistory {
    events: Vec<(CoreEvent, Option<String>)>,
    capacity: usize,
}

impl Default for EventHistory {
    fn default() -> Self {
        Self::new(2048)
    }
}

impl EventHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Vec::new(),
            capacity,
        }
    }

    pub fn merge(&mut self, events: impl IntoIterator<Item = CoreEvent>) {
        for event in events {
            if self
                .events
                .iter()
                .any(|(existing, _)| existing.id == event.id)
            {
                continue;
            }
            let progress_key = matches!(
                event.kind.as_str(),
                "progress" | "copy-progress" | "outboard-progress"
            )
            .then(|| {
                let data: serde_json::Value =
                    serde_json::from_str(&event.data_json).unwrap_or_default();
                serde_json::json!([
                    event.transfer_id,
                    event.direction,
                    event.phase,
                    event.kind,
                    data["connection_id"],
                    data["request_id"]
                ])
                .to_string()
            });
            if let Some(key) = &progress_key {
                // Repeated byte updates must not push preparation sizes and activity out of memory.
                if self.events.iter().any(|(existing, existing_key)| {
                    existing_key.as_ref() == Some(key)
                        && (existing.timestamp, existing.revision)
                            >= (event.timestamp, event.revision)
                }) {
                    continue;
                }
                self.events
                    .retain(|(_, existing_key)| existing_key.as_ref() != Some(key));
            }
            self.events.push((event, progress_key));
        }
        self.events.sort_by(|(a, _), (b, _)| {
            (b.timestamp, b.revision, &b.id).cmp(&(a.timestamp, a.revision, &a.id))
        });
        self.events.truncate(self.capacity);
    }

    pub fn snapshot(&self) -> Vec<CoreEvent> {
        self.events.iter().map(|(event, _)| event.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(timestamp: i64, revision: u64) -> CoreEvent {
        CoreEvent {
            id: format!("{timestamp}-{revision}"),
            timestamp,
            revision,
            scope: "transfer".into(),
            transfer_id: Some(1),
            direction: Some("receive".into()),
            phase: "download".into(),
            kind: "started".into(),
            data_json: "{}".into(),
        }
    }

    #[test]
    fn startup_history_and_racing_callbacks_merge_once_in_order_across_restarts() {
        let mut history = EventHistory::new(4);
        history.merge([event(200, 2)]);
        history.merge([event(100, 99), event(200, 1), event(200, 2)]);
        history.merge([event(200, 3), event(200, 10), event(200, 9)]);
        assert_eq!(
            history
                .snapshot()
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            ["200-10", "200-9", "200-3", "200-2"]
        );
        let snapshot = history.snapshot();
        history.merge([event(300, 1)]);
        assert_eq!(snapshot[0].id, "200-10");
        assert_eq!(history.snapshot()[0].id, "300-1");
    }

    #[test]
    fn repeated_progress_keeps_initial_size_and_meaningful_activity() {
        let mut history = EventHistory::new(4);
        history.merge([event(100, 1)]);
        for revision in 2..100 {
            let mut update = event(100, revision);
            update.kind = "progress".into();
            history.merge([update]);
        }
        let mut stale = event(100, 50);
        stale.kind = "progress".into();
        history.merge([stale]);
        assert_eq!(
            history
                .snapshot()
                .iter()
                .map(|event| event.id.clone())
                .collect::<Vec<_>>(),
            ["100-99", "100-1"]
        );
    }
}
