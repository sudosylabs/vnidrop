use std::collections::{HashMap, HashSet};

use serde_json::Value;
use vnidrop::{CoreEvent, ReceiverRequest, StoredTransfer};

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub label: &'static str,
    pub bytes: Option<u64>,
    pub total: Option<u64>,
}

impl Progress {
    pub fn fraction(&self) -> Option<f64> {
        let (bytes, total) = (self.bytes?, self.total?);
        (total != 0).then(|| (bytes as f64 / total as f64).clamp(0.0, 1.0))
    }
}

pub fn for_transfer(events: &[CoreEvent], transfer: &StoredTransfer) -> Option<Progress> {
    if !matches!(transfer.status.as_str(), "importing" | "receiving") {
        return None;
    }
    let latest = events.iter().find(|event| {
        event.transfer_id == Some(transfer.transfer_id)
            && event.direction.as_deref() == Some(transfer.direction.as_str())
            && matches!(
                event.phase.as_str(),
                "import" | "network" | "handshake" | "download" | "export" | "lifecycle" | "error"
            )
            && matches!(
                event.kind.as_str(),
                "started"
                    | "copy-progress"
                    | "outboard-progress"
                    | "progress"
                    | "found-collection"
                    | "connecting"
                    | "connected"
                    | "done"
                    | "completed"
                    | "failed"
                    | "cancelled"
                    | "aborted"
            )
    })?;
    if matches!(
        latest.kind.as_str(),
        "done" | "completed" | "failed" | "cancelled" | "aborted"
    ) {
        return None;
    }
    let data: Value = serde_json::from_str(&latest.data_json).unwrap_or(Value::Null);
    let bytes = number(
        &data,
        &[
            "exported",
            "downloaded",
            "offset",
            "end_offset",
            "transferred",
            "written",
        ],
    );
    let total = number(&data, &["file_size", "total_size", "size", "total"])
        .or((transfer.total_size != 0).then_some(transfer.total_size));
    let label = match latest.phase.as_str() {
        "import" => "progress_preparing",
        "network" if latest.kind == "connected" => "progress_connected",
        "network" => "progress_connecting",
        "handshake" => "progress_requesting_access",
        "download" if latest.kind == "found-collection" => "progress_getting_ready",
        "download" => "progress_downloading",
        "export" => "progress_saving",
        _ => "progress_working",
    };
    Some(Progress {
        label,
        bytes,
        total,
    })
}

pub fn for_receiver(
    events: &[CoreEvent],
    request: &ReceiverRequest,
    total: u64,
) -> Option<Progress> {
    if request.status != "accepted" {
        return None;
    }
    let endpoint = &request.remote_endpoint_id;
    if endpoint.is_empty() {
        return None;
    }
    // The core emits stable diagnostic fingerprints, never raw endpoint identities.
    let fingerprint = format!(
        "<redacted:{}>",
        &blake3::hash(endpoint.as_bytes()).to_hex()[..8]
    );
    let matches_endpoint = |value: &Value| {
        value
            .as_str()
            .is_some_and(|id| id == endpoint || id == fingerprint)
    };
    let parsed: Vec<_> = events
        .iter()
        .filter(|event| event.timestamp >= request.requested_at)
        .filter_map(|event| {
            serde_json::from_str::<Value>(&event.data_json)
                .ok()
                .map(|data| (event, data))
        })
        .collect();
    let connections: HashSet<_> = parsed
        .iter()
        .filter(|(_, data)| matches_endpoint(&data["endpoint_id"]))
        .filter_map(|(_, data)| identifier(&data["connection_id"]))
        .collect();
    let relevant: Vec<_> = parsed
        .iter()
        .filter(|(event, data)| {
            event.transfer_id == Some(request.transfer_id)
                && event.direction.as_deref() == Some("send")
                && event.phase == "transfer"
                && matches!(
                    event.kind.as_str(),
                    "started" | "progress" | "completed" | "aborted"
                )
                && if data["endpoint_id"].is_null() {
                    identifier(&data["connection_id"]).is_some_and(|id| connections.contains(&id))
                } else {
                    matches_endpoint(&data["endpoint_id"])
                }
        })
        .collect();
    let (latest, _) = relevant.first()?;
    #[derive(Default)]
    struct Blob {
        size: Option<u64>,
        bytes: u64,
        completed: bool,
        aborted: bool,
    }
    let mut blobs: HashMap<(Option<String>, String), Blob> = HashMap::new();
    let mut connection_progress = None;
    for (event, data) in relevant.iter().rev().copied() {
        let Some(id) = identifier(&data["request_id"]) else {
            if let Some(bytes) = number(data, &["end_offset", "offset", "transferred"]) {
                connection_progress = Some((bytes, number(data, &["size"])));
            }
            continue;
        };
        let blob = blobs
            .entry((identifier(&data["connection_id"]), id))
            .or_default();
        if let Some(size) = number(data, &["size"]) {
            blob.size = Some(size);
        }
        match event.kind.as_str() {
            "started" | "progress" => {
                if let Some(bytes) = number(data, &["end_offset", "offset", "transferred"]) {
                    blob.bytes = blob.bytes.max(bytes);
                }
                blob.aborted = false;
            }
            "completed" => {
                blob.completed = true;
            }
            "aborted" => {
                blob.aborted = true;
            }
            _ => unreachable!(),
        }
    }
    let active: Vec<_> = blobs.values().filter(|blob| !blob.aborted).collect();
    if latest.kind == "aborted" && active.iter().all(|blob| blob.completed) {
        return Some(Progress {
            label: "progress_interrupted",
            bytes: None,
            total: None,
        });
    }
    let (bytes, observed_total) = if blobs.is_empty() {
        connection_progress
            .map(|(bytes, total)| (Some(bytes), total))
            .unwrap_or((None, None))
    } else {
        let bytes = active.iter().fold(0u64, |sum, blob| {
            sum.saturating_add(if blob.completed {
                blob.size.unwrap_or(blob.bytes)
            } else {
                blob.bytes.min(blob.size.unwrap_or(u64::MAX))
            })
        });
        let size = active
            .iter()
            .filter_map(|blob| blob.size)
            .fold(0u64, u64::saturating_add);
        (Some(bytes), (size != 0).then_some(size))
    };
    Some(Progress {
        // A completed blob is not a delivery receipt. Keep the receiver's durable status authoritative.
        label: "progress_sending",
        bytes,
        total: (total != 0).then_some(total).or(observed_total),
    })
}

fn number(data: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        data[key]
            .as_u64()
            .or_else(|| data[key].as_str()?.parse().ok())
    })
}

fn identifier(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|id| id.to_string()))
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
