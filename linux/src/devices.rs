use std::collections::BTreeSet;

use vnidrop::{
    DeviceRelationship, DeviceRelationshipState, PairingEligibilitySummary, SavedDevice,
    VnidropCore,
};

use crate::error::{message_key, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Devices {
    pub saved: Vec<SavedDevice>,
    pub relationships: Vec<DeviceRelationship>,
    pub eligible: Vec<PairingEligibilitySummary>,
    pub blocked: Vec<String>,
    pub history_peers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceState {
    Eligible { session: String },
    Incoming { generation: u64 },
    Outgoing { generation: u64 },
    Saved { generation: u64 },
    Blocked,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub peer: String,
    pub name: Option<String>,
    pub remote_name: Option<String>,
    pub label: Option<String>,
    pub state: DeviceState,
}

#[derive(Clone, Debug)]
pub enum DeviceAction {
    Remember,
    Decline,
    Accept,
    Label(String),
    Forget,
    Block,
    Unblock,
}

impl Devices {
    pub fn read(core: &VnidropCore) -> std::result::Result<Self, vnidrop::VnidropError> {
        Ok(Self {
            saved: core.list_saved_devices()?,
            relationships: core.list_device_relationships()?,
            eligible: core.list_pairing_eligibilities()?,
            blocked: core.list_blocked_devices()?,
            history_peers: Vec::new(),
        })
    }

    pub fn list(&self, now: i64) -> Vec<Device> {
        let peers: BTreeSet<_> = self
            .saved
            .iter()
            .map(|d| &d.endpoint_id)
            .chain(self.relationships.iter().map(|d| &d.remote_endpoint_id))
            .chain(self.eligible.iter().map(|d| &d.peer_endpoint_id))
            .chain(self.blocked.iter())
            .chain(self.history_peers.iter())
            .collect();
        let mut devices: Vec<_> = peers
            .into_iter()
            .filter_map(|peer| {
                let saved = self.saved.iter().find(|d| d.endpoint_id == *peer);
                let relationship = self
                    .relationships
                    .iter()
                    .find(|d| d.remote_endpoint_id == *peer);
                let eligible = self
                    .eligible
                    .iter()
                    .find(|d| d.peer_endpoint_id == *peer && d.expires_at > now);
                let state = if self.blocked.contains(peer) {
                    DeviceState::Blocked
                } else if let Some(relationship) = relationship {
                    match relationship.state {
                        DeviceRelationshipState::PendingIncoming => DeviceState::Incoming {
                            generation: relationship.generation,
                        },
                        DeviceRelationshipState::PendingOutgoing => DeviceState::Outgoing {
                            generation: relationship.generation,
                        },
                        DeviceRelationshipState::Saved => DeviceState::Saved {
                            generation: relationship.generation,
                        },
                        DeviceRelationshipState::Blocked => DeviceState::Blocked,
                        DeviceRelationshipState::Revoked => match eligible {
                            Some(eligible) => DeviceState::Eligible {
                                session: eligible.session_id.clone(),
                            },
                            None if self.history_peers.contains(peer) => DeviceState::Unavailable,
                            None => return None,
                        },
                    }
                } else if eligible.is_none() && self.history_peers.contains(peer) {
                    DeviceState::Unavailable
                } else if saved.is_some() {
                    return None;
                } else {
                    DeviceState::Eligible {
                        session: eligible?.session_id.clone(),
                    }
                };
                let label = saved.and_then(|d| nonblank(d.local_label.as_deref()));
                let remote_name = saved
                    .and_then(|d| nonblank(d.remote_display_name.as_deref()))
                    .or_else(|| eligible.and_then(|d| nonblank(d.remote_display_name.as_deref())));
                Some(Device {
                    peer: peer.clone(),
                    name: label.clone().or_else(|| remote_name.clone()),
                    remote_name,
                    label,
                    state,
                })
            })
            .collect();
        devices.sort_by(|a, b| {
            a.priority()
                .cmp(&b.priority())
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.peer.cmp(&b.peer))
        });
        devices
    }
}

impl Device {
    fn priority(&self) -> u8 {
        match self.state {
            DeviceState::Incoming { .. } => 0,
            DeviceState::Eligible { .. } => 1,
            DeviceState::Outgoing { .. } => 2,
            DeviceState::Saved { .. } => 3,
            DeviceState::Blocked => 4,
            DeviceState::Unavailable => 5,
        }
    }

    pub fn status_key(&self) -> &'static str {
        match self.state {
            DeviceState::Incoming { .. } => "saved_devices_pending_incoming",
            DeviceState::Outgoing { .. } => "saved_devices_pending_outgoing",
            DeviceState::Eligible { .. } => "saved_devices_eligibility_title",
            DeviceState::Saved { .. } => "saved_devices_list_title",
            DeviceState::Blocked => "saved_devices_blocked",
            DeviceState::Unavailable => "linux_device_unavailable",
        }
    }

    pub fn needs_attention(&self) -> bool {
        matches!(
            self.state,
            DeviceState::Incoming { .. } | DeviceState::Eligible { .. }
        )
    }
}

/// Re-read consent state before applying a decision from a possibly stale dialog.
pub fn execute(core: &VnidropCore, target: &Device, action: DeviceAction, now: i64) -> Result<()> {
    let current = Devices::read(core)
        .map_err(message_key)?
        .list(now)
        .into_iter()
        .find(|d| d.peer == target.peer)
        .ok_or("linux_device_changed")?;
    if current.state != target.state {
        return Err("linux_device_changed");
    }
    let peer = target.peer.clone();
    match (&current.state, action) {
        (DeviceState::Eligible { .. }, DeviceAction::Remember) => core
            .request_saved_device_pairing(peer)
            .map_err(message_key)?
            .then_some(())
            .ok_or("linux_device_changed"),
        (DeviceState::Eligible { .. }, DeviceAction::Decline) => {
            core.decline_pairing_eligibility(peer).map_err(message_key)
        }
        (DeviceState::Incoming { .. }, DeviceAction::Accept) => core
            .respond_to_device_pairing(peer, true)
            .map_err(message_key)?
            .then_some(())
            .ok_or("linux_device_changed"),
        (DeviceState::Incoming { .. }, DeviceAction::Decline) => core
            .respond_to_device_pairing(peer, false)
            .map_err(message_key)?
            .then_some(())
            .ok_or("linux_device_changed"),
        (DeviceState::Saved { .. }, DeviceAction::Label(label)) => core
            .set_saved_device_label(peer, nonblank(Some(&label)))
            .map_err(message_key),
        (DeviceState::Saved { .. }, DeviceAction::Forget) => {
            core.forget_saved_device(peer).map_err(message_key)
        }
        (DeviceState::Saved { .. }, DeviceAction::Block) => {
            core.block_device(peer).map_err(message_key)
        }
        (DeviceState::Blocked, DeviceAction::Unblock) => {
            core.unblock_device(peer).map_err(message_key)
        }
        _ => Err("linux_device_changed"),
    }
}

fn nonblank(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod tests;
