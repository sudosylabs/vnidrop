use crate::{devices::DeviceState, session::Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Transfer(u64, String, Option<String>),
    Device(String, Option<String>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub id: String,
    pub title: &'static str,
    pub body: String,
    pub target: Target,
    pub pending: bool,
}
#[derive(Default)]
pub struct Tracker {
    previous: Option<BTreeMap<String, String>>,
    published: BTreeMap<String, String>,
}
impl Tracker {
    pub fn withdraw_pending(&mut self) -> Vec<String> {
        let pending: Vec<_> = self
            .published
            .iter()
            .filter(|(_, state)| {
                matches!(state.as_str(), "requested" | "pending") || state.starts_with("Incoming")
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &pending {
            self.published.remove(id);
        }
        pending
    }
    pub fn update(
        &mut self,
        snapshot: &Snapshot,
        enabled: bool,
        foreground: bool,
        now: i64,
    ) -> (Vec<Notice>, Vec<String>) {
        let mut states = BTreeMap::new();
        let mut candidates = Vec::new();
        for request in &snapshot.requests {
            let id = format!("receiver:{}", request.id);
            states.insert(id.clone(), request.status.clone());
            let transfer = snapshot
                .transfers
                .iter()
                .find(|t| t.transfer_id == request.transfer_id && t.direction == "send");
            let name = transfer
                .and_then(|t| t.transfer_name.clone())
                .unwrap_or_default();
            let title = match request.status.as_str() {
                "requested" => Some("approval_connection_request"),
                "completed" => Some("notifications_receiver_completed_title"),
                "failed" => Some("notifications_receiver_failed_title"),
                _ => None,
            };
            if let Some(title) = title {
                candidates.push(Notice {
                    id,
                    title,
                    body: name,
                    target: Target::Transfer(
                        request.transfer_id,
                        "send".into(),
                        Some(request.id.clone()),
                    ),
                    pending: request.status == "requested",
                });
            }
        }
        for transfer in &snapshot.transfers {
            let id = format!("transfer:{}:{}", transfer.transfer_id, transfer.direction);
            states.insert(id.clone(), transfer.status.clone());
            let title = match (transfer.direction.as_str(), transfer.status.as_str()) {
                ("receive", "completed") => Some("notifications_receive_completed_title"),
                ("receive", "failed") => Some("notifications_receive_failed_title"),
                ("send", "failed") => Some("notifications_send_failed_title"),
                _ => None,
            };
            if let Some(title) = title {
                candidates.push(Notice {
                    id,
                    title,
                    body: transfer.transfer_name.clone().unwrap_or_default(),
                    target: Target::Transfer(
                        transfer.transfer_id,
                        transfer.direction.clone(),
                        None,
                    ),
                    pending: false,
                });
            }
        }
        for offer in &snapshot.offers {
            let id = format!("offer:{}", offer.transfer_id);
            states.insert(id.clone(), "pending".into());
            candidates.push(Notice {
                id,
                title: "saved_devices_transfers_title",
                body: offer.transfer_name.clone(),
                target: Target::Device(
                    offer.sender_endpoint_id.clone(),
                    Some(offer.transfer_id.clone()),
                ),
                pending: true,
            });
        }
        for transfer in &snapshot.targeted {
            let id = format!("direct:{}", transfer.id);
            states.insert(id.clone(), format!("{:?}", transfer.state));
            let title = match transfer.state {
                vnidrop::TargetedTransferState::Completed => Some("status_completed"),
                vnidrop::TargetedTransferState::Failed => Some("status_failed"),
                _ => None,
            };
            if let Some(title) = title {
                candidates.push(Notice {
                    id,
                    title,
                    body: transfer.transfer_name.clone(),
                    target: Target::Device(crate::targeted::peer(transfer).into(), None),
                    pending: false,
                });
            }
        }
        for device in snapshot.devices.list(now) {
            let id = format!("device:{}", device.peer);
            states.insert(id.clone(), format!("{:?}", device.state));
            if matches!(device.state, DeviceState::Incoming { .. }) {
                candidates.push(Notice {
                    id,
                    title: "saved_devices_attention_title",
                    body: device.name.unwrap_or_default(),
                    target: Target::Device(device.peer, None),
                    pending: true,
                });
            }
        }
        let live: BTreeSet<_> = candidates.iter().map(|n| n.id.clone()).collect();
        let withdrawn: Vec<_> = self
            .published
            .iter()
            .filter(|(id, state)| {
                !enabled || foreground || !live.contains(*id) || states.get(*id) != Some(*state)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &withdrawn {
            self.published.remove(id);
        }
        let send: Vec<_> = candidates
            .into_iter()
            .filter(|n| {
                enabled
                    && !foreground
                    && !self.published.contains_key(&n.id)
                    && (n.pending
                        || self
                            .previous
                            .as_ref()
                            .is_some_and(|p| p.get(&n.id) != states.get(&n.id)))
            })
            .collect();
        for notice in &send {
            self.published
                .insert(notice.id.clone(), states[&notice.id].clone());
        }
        self.previous = Some(states);
        (send, withdrawn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_lifecycle_deduplicates_withdraws_and_ignores_old_completions() {
        let mut snapshot = Snapshot {
            transfers: vec![],
            requests: vec![],
            artifacts: vec![],
            events: vec![],
            targeted: vec![],
            offers: vec![],
            devices: Default::default(),
            obligations: vnidrop::RuntimeObligationFacts {
                active_invitation_transfers: 0,
                invitation_provider_availability: 0,
                targeted_preparations: 0,
                active_targeted_transfers: 0,
                targeted_provider_availability: 0,
            },
        };
        snapshot.targeted.push(vnidrop::TargetedTransfer {
            id: "one".into(),
            role: vnidrop::TargetedTransferRole::Receiver,
            sender_endpoint_id: "sender".into(),
            receiver_endpoint_id: "receiver".into(),
            manifest_id: "manifest".into(),
            transfer_name: "Files".into(),
            file_count: 1,
            total_size: 1,
            verified_bytes: 1,
            state: vnidrop::TargetedTransferState::Completed,
            created_at: 1,
            updated_at: 1,
        });
        let mut tracker = Tracker::default();
        assert!(tracker.update(&snapshot, true, false, 0).0.is_empty());
        snapshot.targeted[0].state = vnidrop::TargetedTransferState::Transferring;
        tracker.update(&snapshot, true, false, 0);
        snapshot.targeted[0].state = vnidrop::TargetedTransferState::Completed;
        let (sent, _) = tracker.update(&snapshot, true, false, 0);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].target, Target::Device("sender".into(), None));
        assert!(tracker.update(&snapshot, true, false, 0).0.is_empty());
        assert_eq!(
            tracker.update(&snapshot, true, true, 0).1,
            vec!["direct:one"]
        );
        assert!(tracker.update(&snapshot, true, false, 0).0.is_empty());
        snapshot.offers.push(vnidrop::PendingTargetedOffer {
            transfer_id: "offer".into(),
            sender_endpoint_id: "sender".into(),
            receiver_endpoint_id: "receiver".into(),
            manifest_id: "manifest".into(),
            content_hash: "hash".into(),
            transfer_name: "Files".into(),
            file_count: 1,
            total_size: 1,
            protocol_version: 1,
            received_at: 1,
        });
        assert_eq!(tracker.update(&snapshot, true, false, 0).0.len(), 1);
        assert_eq!(
            tracker.update(&snapshot, false, false, 0).1,
            vec!["offer:offer"]
        );
        assert!(tracker.update(&snapshot, false, false, 0).0.is_empty());
        assert_eq!(tracker.update(&snapshot, true, false, 0).0.len(), 1);
        assert_eq!(tracker.withdraw_pending(), vec!["offer:offer"]);
        assert_eq!(tracker.update(&snapshot, true, false, 0).0.len(), 1);
        snapshot.offers.clear();
        assert_eq!(
            tracker.update(&snapshot, true, false, 0).1,
            vec!["offer:offer"]
        );
    }
}
