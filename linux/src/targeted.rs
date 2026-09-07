use crate::{
    devices::{Device, Devices},
    error::{message_key, Result},
    session::Session,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use vnidrop::{
    TargetedPreparationStopOutcome, TargetedTransfer, TargetedTransferRole as Role,
    TargetedTransferState as State,
};

pub fn peer(transfer: &TargetedTransfer) -> &str {
    match transfer.role {
        Role::Sender => &transfer.receiver_endpoint_id,
        Role::Receiver => &transfer.sender_endpoint_id,
    }
}

pub fn status_key(state: State) -> &'static str {
    match state {
        State::Preparing => "status_preparing",
        State::Offering => "linux_targeted_offering",
        State::AwaitingApproval => "status_awaiting_approval",
        State::Approved => "transfer_receiver_accepted",
        State::Connecting => "progress_connecting",
        State::Transferring => "linux_targeted_transferring",
        State::Interrupted => "progress_interrupted",
        State::Completed => "status_completed",
        State::Declined => "transfer_receiver_refused",
        State::Cancelled => "status_cancelled",
        State::Failed => "status_failed",
        State::Deleted => "linux_targeted_deleted",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Receive,
    Resume,
    Cancel,
    Delete,
}

pub fn actions(transfer: &TargetedTransfer) -> Vec<Action> {
    let mut actions = Vec::new();
    if transfer.role == Role::Receiver {
        match transfer.state {
            State::Approved => actions.push(Action::Receive),
            State::Interrupted => actions.push(Action::Resume),
            _ => {}
        }
    }
    match transfer.state {
        State::Preparing
        | State::Offering
        | State::AwaitingApproval
        | State::Approved
        | State::Connecting
        | State::Transferring
        | State::Interrupted => actions.push(Action::Cancel),
        State::Completed | State::Declined | State::Cancelled | State::Failed => {
            actions.push(Action::Delete)
        }
        State::Deleted => {}
    }
    actions
}

/// Holds cancellation intent until the core's preparation handle is available.
pub struct Preparation {
    cancelled: AtomicBool,
    handle: Mutex<Option<Arc<vnidrop::TargetedTransferPreparation>>>,
}

impl Preparation {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            handle: Mutex::new(None),
        })
    }
    pub fn request_cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn stop(&self) -> Result<Option<TargetedPreparationStopOutcome>> {
        self.request_cancel();
        let handle = self.handle.lock().unwrap().clone();
        handle
            .map(|handle| handle.stop().map_err(message_key))
            .transpose()
    }
    pub fn run(
        &self,
        session: &Session,
        target: &Device,
        submission: crate::composer::Submission,
    ) -> Result<TargetedTransfer> {
        if self.is_cancelled() {
            return Err("progress_cancelled");
        }
        let handle = session
            .call(|core| {
                let current = Devices::read(core)?
                    .list(0)
                    .into_iter()
                    .find(|device| device.peer == target.peer);
                if current.as_ref().map(|d| &d.state) != Some(&target.state) {
                    return Ok(None);
                }
                core.new_targeted_transfer_preparation(target.peer.clone())
                    .map(Some)
            })?
            .ok_or("linux_device_changed")?;
        self.handle.lock().unwrap().replace(handle.clone());
        if self.is_cancelled() {
            self.stop()?;
            return Err("progress_cancelled");
        }
        let result =
            session.call(|_| handle.send(submission.sources, Some(submission.transfer_name)));
        if self.is_cancelled() {
            self.stop()?;
            return Err("progress_cancelled");
        }
        result
    }
}
