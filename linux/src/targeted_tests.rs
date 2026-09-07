use super::*;
use crate::{
    devices::{Device, DeviceState},
    targeted::{self, Action, Preparation as TargetedPreparation},
};
use vnidrop::{
    TargetedOfferResponse, TargetedPreparationStopOutcome, TargetedTransferRole as Role,
    TargetedTransferState as State,
};

fn wait_snapshot(session: &Session, check: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let snapshot = session.snapshot().unwrap();
        if check(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "direct transfer did not reach the expected state"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn pair(sender: &Arc<Session>, receiver: &Arc<Session>, root: &Path) -> Device {
    let source = root.join("pair.txt");
    std::fs::write(&source, b"pairing").unwrap();
    let output = root.join("pair-output");
    std::fs::create_dir(&output).unwrap();
    let share = sender
        .call(|core| core.share_files(crate::draft::sources(vec![source]).unwrap(), metadata(940)))
        .unwrap();
    let peer = receiver.clone();
    let worker = thread::spawn(move || {
        peer.call(|core| {
            core.receive(
                share.ticket,
                output.to_str().unwrap().into(),
                Some("Receiver".into()),
            )
        })
    });
    let request = wait_request(sender, 940);
    sender
        .call(|core| core.respond_receiver_request(request.id, true, None))
        .unwrap();
    worker.join().unwrap().unwrap();
    let eligible = wait_device(sender, |d| matches!(d.state, DeviceState::Eligible { .. }));
    sender
        .call(|core| core.request_saved_device_pairing(eligible.peer))
        .unwrap();
    let incoming = wait_device(receiver, |d| {
        matches!(d.state, DeviceState::Incoming { .. })
    });
    receiver
        .call(|core| core.respond_to_device_pairing(incoming.peer, true))
        .unwrap();
    wait_device(receiver, |d| matches!(d.state, DeviceState::Saved { .. }));
    wait_device(sender, |d| matches!(d.state, DeviceState::Saved { .. }))
}

fn submission(source: &Path) -> crate::composer::Submission {
    crate::composer::Submission {
        sources: crate::draft::sources(vec![source.into()]).unwrap(),
        transfer_name: "Direct files".into(),
        sender_name: "unused".into(),
        access_mode: TransferAccessMode::ApprovalRequired,
    }
}

#[test]
fn native_direct_transfer_handles_consent_delivery_resume_and_revocation() {
    let root = tempfile::tempdir().unwrap();
    let sender = session(&root.path().join("sender"));
    let receiver = session(&root.path().join("receiver"));
    let saved = pair(&sender, &receiver, root.path());
    let source = root.path().join("direct.txt");
    std::fs::write(&source, b"native direct transfer").unwrap();
    let cancelled = TargetedPreparation::new();
    cancelled.request_cancel();
    assert_eq!(
        cancelled
            .run(&sender, &saved, submission(&source))
            .unwrap_err(),
        "progress_cancelled"
    );
    assert!(sender.snapshot().unwrap().targeted.is_empty());
    let mut stale = saved.clone();
    if let DeviceState::Saved { generation } = &mut stale.state {
        *generation += 1;
    }
    assert_eq!(
        TargetedPreparation::new()
            .run(&sender, &stale, submission(&source))
            .unwrap_err(),
        "linux_device_changed"
    );
    let transfer = TargetedPreparation::new()
        .run(&sender, &saved, submission(&source))
        .unwrap();
    let pending = wait_snapshot(&receiver, |s| {
        s.offers.iter().any(|o| o.transfer_id == transfer.id)
    });
    assert_eq!(pending.offers[0].file_count, 1);
    assert_eq!(pending.offers[0].total_size, 22);
    assert!(receiver
        .snapshot()
        .unwrap()
        .targeted
        .iter()
        .all(|t| t.state != State::Transferring));
    assert!(matches!(
        receiver
            .call(|core| core.respond_to_targeted_offer(transfer.id.clone(), true))
            .unwrap(),
        TargetedOfferResponse::Approved { .. }
    ));
    let invalid = root.path().join("not-a-folder");
    std::fs::write(&invalid, b"keep").unwrap();
    assert!(receiver
        .call(|core| core
            .receive_targeted_transfer(transfer.id.clone(), invalid.to_str().unwrap().into()))
        .is_err());
    let interrupted = wait_snapshot(&receiver, |s| {
        s.targeted
            .iter()
            .any(|t| t.id == transfer.id && t.state == State::Interrupted)
    });
    let interrupted = interrupted
        .targeted
        .iter()
        .find(|t| t.id == transfer.id)
        .unwrap();
    assert_eq!(
        targeted::actions(interrupted),
        vec![Action::Resume, Action::Cancel]
    );
    let destination = root.path().join("direct-output");
    std::fs::create_dir(&destination).unwrap();
    receiver
        .call(|core| {
            core.resume_targeted_transfer(transfer.id.clone(), destination.to_str().unwrap().into())
        })
        .unwrap();
    wait_snapshot(&receiver, |s| {
        s.targeted
            .iter()
            .any(|t| t.id == transfer.id && t.state == State::Completed)
    });
    wait_snapshot(&sender, |s| {
        s.targeted
            .iter()
            .any(|t| t.id == transfer.id && t.state == State::Completed)
    });
    fn has_bytes(path: &Path) -> bool {
        std::fs::read_dir(path).unwrap().any(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                has_bytes(&path)
            } else {
                std::fs::read(path).unwrap() == b"native direct transfer"
            }
        })
    }
    assert!(has_bytes(&destination));
    let declined = TargetedPreparation::new()
        .run(&sender, &saved, submission(&source))
        .unwrap();
    wait_snapshot(&receiver, |s| {
        s.offers.iter().any(|o| o.transfer_id == declined.id)
    });
    assert_eq!(
        receiver
            .call(|core| core.respond_to_targeted_offer(declined.id.clone(), false))
            .unwrap(),
        TargetedOfferResponse::Declined
    );
    wait_snapshot(&sender, |s| {
        s.targeted
            .iter()
            .any(|t| t.id == declined.id && t.state == State::Declined)
    });
    let preparation = TargetedPreparation::new();
    let cancelled = preparation
        .run(&sender, &saved, submission(&source))
        .unwrap();
    assert!(matches!(
        preparation.stop().unwrap(),
        Some(
            TargetedPreparationStopOutcome::TransferAbandoned
                | TargetedPreparationStopOutcome::TransferCancelled
                | TargetedPreparationStopOutcome::AlreadyTerminal
        )
    ));
    wait_snapshot(&sender, |s| {
        s.targeted.iter().all(|t| {
            t.id != cancelled.id
                || matches!(t.state, State::Cancelled | State::Deleted | State::Failed)
        })
    });
    sender
        .call(|core| core.forget_saved_device(saved.peer.clone()))
        .unwrap();
    assert_eq!(
        TargetedPreparation::new()
            .run(&sender, &saved, submission(&source))
            .unwrap_err(),
        "linux_device_changed"
    );
    assert!(sender
        .snapshot()
        .unwrap()
        .devices
        .list(0)
        .iter()
        .any(|d| d.peer == saved.peer && matches!(d.state, DeviceState::Unavailable)));
    sender
        .call(|core| core.delete_targeted_transfer(transfer.id.clone()))
        .unwrap();
    assert!(sender
        .snapshot()
        .unwrap()
        .targeted
        .iter()
        .all(|t| t.id != transfer.id || t.state == State::Deleted));
    receiver.close();
    sender.close();
}

#[test]
fn direct_transfer_actions_follow_role_and_durable_state() {
    let mut transfer = vnidrop::TargetedTransfer {
        id: "test".into(),
        role: Role::Receiver,
        sender_endpoint_id: "sender".into(),
        receiver_endpoint_id: "receiver".into(),
        manifest_id: "manifest".into(),
        transfer_name: "files".into(),
        file_count: 1,
        total_size: 1,
        verified_bytes: 0,
        state: State::Approved,
        created_at: 0,
        updated_at: 0,
    };
    assert_eq!(
        targeted::actions(&transfer),
        vec![Action::Receive, Action::Cancel]
    );
    transfer.role = Role::Sender;
    assert_eq!(targeted::actions(&transfer), vec![Action::Cancel]);
    for state in [
        State::Completed,
        State::Cancelled,
        State::Declined,
        State::Failed,
    ] {
        transfer.state = state;
        assert_eq!(targeted::actions(&transfer), vec![Action::Delete]);
    }
    transfer.state = State::Deleted;
    assert!(targeted::actions(&transfer).is_empty());
}
