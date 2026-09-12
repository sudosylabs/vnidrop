use crate::{
    devices::{Device, DeviceState},
    localization::{format, text},
    session::Snapshot,
    targeted,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn device_name(devices: &[Device], peer: &str) -> String {
    devices
        .iter()
        .find(|device| device.peer == peer)
        .and_then(|device| device.name.clone())
        .unwrap_or_else(|| text("approval_nearby_device"))
}

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
        let devices = snapshot.devices.list(now);
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
            let name = device_name(&devices, &offer.sender_endpoint_id);
            candidates.push(Notice {
                id,
                title: "targeted_offer_title",
                body: format(
                    "targeted_offer_body",
                    &[("device", &name), ("transferName", &offer.transfer_name)],
                ),
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
            let peer = targeted::peer(transfer);
            if let Some(copy) = targeted::notice_copy(transfer, &device_name(&devices, peer)) {
                candidates.push(Notice {
                    id,
                    title: copy.title,
                    body: copy.body,
                    target: Target::Device(peer.into(), None),
                    pending: false,
                });
            }
        }
        for device in &devices {
            let id = format!("device:{}", device.peer);
            states.insert(id.clone(), format!("{:?}", device.state));
            if matches!(device.state, DeviceState::Incoming { .. }) {
                let name = device
                    .name
                    .clone()
                    .unwrap_or_else(|| text("approval_nearby_device"));
                candidates.push(Notice {
                    id,
                    title: "pairing_request_title",
                    body: format("pairing_request_body", &[("device", &name)]),
                    target: Target::Device(device.peer.clone(), None),
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
    use vnidrop::{
        DeviceRelationship, DeviceRelationshipState, SavedDevice, TargetedTransfer,
        TargetedTransferRole, TargetedTransferState,
    };

    fn empty_snapshot() -> Snapshot {
        Snapshot {
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
        }
    }

    fn transfer(role: TargetedTransferRole, state: TargetedTransferState) -> TargetedTransfer {
        TargetedTransfer {
            id: "one".into(),
            role,
            sender_endpoint_id: "sender".into(),
            receiver_endpoint_id: "receiver".into(),
            manifest_id: "manifest".into(),
            transfer_name: "Files".into(),
            file_count: 1,
            total_size: 1,
            verified_bytes: 1,
            state,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn devices(peer: &str, state: DeviceRelationshipState) -> crate::devices::Devices {
        crate::devices::Devices {
            saved: vec![SavedDevice {
                endpoint_id: peer.into(),
                local_label: Some(" Desk ".into()),
                remote_display_name: Some("Remote".into()),
                created_at: 1,
                last_authenticated_at: None,
            }],
            relationships: vec![DeviceRelationship {
                remote_endpoint_id: peer.into(),
                state,
                generation: 1,
                minimum_protocol_version: 2,
                created_at: 1,
                updated_at: 1,
            }],
            ..Default::default()
        }
    }

    fn emit(role: TargetedTransferRole, state: TargetedTransferState) -> Notice {
        let mut snapshot = empty_snapshot();
        snapshot.devices = devices(
            match role {
                TargetedTransferRole::Receiver => "sender",
                TargetedTransferRole::Sender => "receiver",
            },
            DeviceRelationshipState::Saved,
        );
        snapshot.targeted.push(transfer(role, state));
        let mut tracker = Tracker::default();
        snapshot.targeted[0].state = TargetedTransferState::Transferring;
        tracker.update(&snapshot, true, false, 0);
        snapshot.targeted[0].state = state;
        let sent = tracker.update(&snapshot, true, false, 0).0;
        assert_eq!(sent.len(), 1);
        sent.into_iter().next().unwrap()
    }

    #[test]
    fn notification_lifecycle_deduplicates_withdraws_and_ignores_old_completions() {
        let mut snapshot = empty_snapshot();
        snapshot.targeted.push(transfer(
            TargetedTransferRole::Receiver,
            TargetedTransferState::Completed,
        ));
        let mut tracker = Tracker::default();
        assert!(tracker.update(&snapshot, true, false, 0).0.is_empty());
        snapshot.targeted[0].state = TargetedTransferState::Transferring;
        tracker.update(&snapshot, true, false, 0);
        snapshot.targeted[0].state = TargetedTransferState::Completed;
        let (sent, _) = tracker.update(&snapshot, true, false, 0);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "notifications_receive_completed_title");
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

    #[test]
    fn sender_completed_is_not_a_receive_completed_notice() {
        let incoming = emit(
            TargetedTransferRole::Receiver,
            TargetedTransferState::Completed,
        );
        let outgoing = emit(
            TargetedTransferRole::Sender,
            TargetedTransferState::Completed,
        );
        assert_eq!(incoming.title, "notifications_receive_completed_title");
        assert_eq!(outgoing.title, "notifications_receiver_completed_title");
        assert_ne!(outgoing.title, "notifications_receive_completed_title");
        assert!(outgoing.body.contains("Desk"));
        assert!(!outgoing.body.contains("Remote"));
        assert_eq!(
            emit(TargetedTransferRole::Sender, TargetedTransferState::Failed).title,
            "notifications_receiver_failed_title"
        );
        assert_eq!(
            emit(
                TargetedTransferRole::Receiver,
                TargetedTransferState::Failed
            )
            .title,
            "notifications_receive_failed_title"
        );
    }

    fn offer() -> vnidrop::PendingTargetedOffer {
        vnidrop::PendingTargetedOffer {
            transfer_id: "offer".into(),
            sender_endpoint_id: "sender".into(),
            receiver_endpoint_id: "receiver".into(),
            manifest_id: "manifest".into(),
            content_hash: "hash".into(),
            transfer_name: "Photos".into(),
            file_count: 1,
            total_size: 1,
            protocol_version: 1,
            received_at: 1,
        }
    }

    #[test]
    fn offers_and_pairing_use_device_names_and_withdraw_when_gone() {
        let mut snapshot = empty_snapshot();
        snapshot.devices = devices("sender", DeviceRelationshipState::Saved);
        snapshot.offers.push(offer());
        let mut tracker = Tracker::default();
        let (sent, _) = tracker.update(&snapshot, true, false, 0);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "targeted_offer_title");
        assert!(sent[0].body.contains("Desk"));
        assert!(sent[0].body.contains("Photos"));
        assert!(!sent[0].body.contains("Remote"));
        snapshot.devices.saved[0].local_label = Some(" ".into());
        snapshot.offers.clear();
        let (sent, withdrawn) = tracker.update(&snapshot, true, false, 0);
        assert_eq!(withdrawn, vec!["offer:offer"]);
        assert!(sent.is_empty());
        snapshot.devices.relationships[0].state = DeviceRelationshipState::PendingIncoming;
        let (sent, _) = tracker.update(&snapshot, true, false, 0);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "pairing_request_title");
        assert!(sent[0].body.contains("Remote"));
        assert_eq!(tracker.withdraw_pending(), vec!["device:sender"]);
        assert_eq!(tracker.update(&snapshot, true, false, 0).0.len(), 1);
        snapshot.devices.relationships.clear();
        snapshot.devices.saved.clear();
        let (sent, withdrawn) = tracker.update(&snapshot, true, false, 0);
        assert!(sent.is_empty());
        assert_eq!(withdrawn, vec!["device:sender"]);
        let mut remote = empty_snapshot();
        remote.devices = devices("sender", DeviceRelationshipState::Saved);
        remote.devices.saved[0].local_label = Some(" ".into());
        remote.offers.push(offer());
        let sent = Tracker::default().update(&remote, true, false, 0).0;
        assert!(sent[0].body.contains("Remote"));
        assert!(sent[0].body.contains("Photos"));
    }
}
