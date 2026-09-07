use std::{
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use vnidrop::{
    CoreLimits, CoreRelayMode, ShareMetadataInput, ShareSource, SourceKind, TransferAccessMode,
};

use super::*;
use crate::draft::Preparation;

fn session(path: &Path) -> Arc<Session> {
    let (sender, changes) = async_channel::bounded(1);
    let events = Arc::new(Mutex::new(EventHistory::default()));
    let core = VnidropCore::initialize_for_integration_test(
        path.to_str().unwrap().into(),
        Arc::new(EventSink(sender, events.clone())),
        CoreLimits::default(),
        CoreNetworkConfig {
            mode: CoreRelayMode::LocalOnly,
            relay_urls: vec![],
        },
    )
    .unwrap();
    Session::with_core(core, changes, events).unwrap()
}

fn metadata(id: u64) -> ShareMetadataInput {
    ShareMetadataInput {
        transfer_id: id,
        transfer_name: Some("Documents".into()),
        sender_name: Some("Sender".into()),
        access_mode: TransferAccessMode::ApprovalRequired,
    }
}

fn wait_request(sender: &Session, id: u64) -> ReceiverRequest {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(request) = sender
            .snapshot()
            .unwrap()
            .requests
            .into_iter()
            .find(|request| request.transfer_id == id && request.status == "requested")
        {
            return request;
        }
        assert!(
            Instant::now() < deadline,
            "receiver did not reach approval within 10 seconds"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn approval_and_cancel_are_available_while_receive_blocks() {
    let root = tempfile::tempdir().unwrap();
    let sender = session(&root.path().join("sender"));
    let receiver = session(&root.path().join("receiver"));
    let path = root.path().join("résumé.txt");
    std::fs::write(&path, b"verified bytes").unwrap();
    let share = sender
        .call(|core| {
            core.share_files(
                vec![ShareSource {
                    kind: SourceKind::Path,
                    value: path.to_str().unwrap().into(),
                    display_name: None,
                    is_directory: false,
                }],
                metadata(120),
            )
        })
        .unwrap();
    let output = root.path().join("received");
    std::fs::create_dir_all(&output).unwrap();
    let receiving = receiver.clone();
    let ticket = share.ticket.clone();
    let destination = output.to_str().unwrap().to_owned();
    let (done, completion) = mpsc::channel();
    thread::spawn(move || {
        done.send(receiving.call(|core| core.receive(ticket, destination, Some("Receiver".into()))))
            .unwrap()
    });
    let request = wait_request(&sender, 120);
    sender
        .call(|core| core.respond_receiver_request(request.id, true, None))
        .unwrap();
    completion
        .recv_timeout(Duration::from_secs(10))
        .expect("approval was blocked behind receive")
        .unwrap();
    assert_eq!(
        std::fs::read(output.join("résumé.txt")).unwrap(),
        b"verified bytes"
    );
    assert_eq!(receiver.snapshot().unwrap().transfers[0].status, "done");
    assert_eq!(receiver.snapshot().unwrap().artifacts.len(), 1);
    assert!(
        receiver.snapshot().unwrap().events.iter().any(|event| {
            event.transfer_id == Some(120)
                && event.direction.as_deref() == Some("receive")
                && event.phase == "lifecycle"
                && event.kind == "done"
        }),
        "live callbacks retain completion history alongside durable state"
    );
    let transfer = sender.snapshot().unwrap().transfers.remove(0);
    assert_eq!(
        crate::presentation::invitation(&transfer),
        Some(share.ticket.as_str())
    );
    sender.call(|core| core.cancel_transfer(120)).unwrap();
    let stopped = sender.snapshot().unwrap().transfers.remove(0);
    assert!(!crate::presentation::is_active(&stopped));
    assert!(
        crate::presentation::invitation(&stopped).is_none(),
        "stopped shares must not expose stale invitations"
    );

    let second = sender
        .call(|core| {
            core.share_files(
                vec![ShareSource {
                    kind: SourceKind::Path,
                    value: path.to_str().unwrap().into(),
                    display_name: None,
                    is_directory: false,
                }],
                metadata(121),
            )
        })
        .unwrap();
    let receiving = receiver.clone();
    let destination = output.to_str().unwrap().to_owned();
    let (done, completion) = mpsc::channel();
    thread::spawn(move || {
        done.send(
            receiving
                .call(|core| core.receive(second.ticket, destination, Some("Receiver".into()))),
        )
        .unwrap()
    });
    wait_request(&sender, 121);
    receiver.call(|core| core.cancel_transfer(121)).unwrap();
    assert!(completion
        .recv_timeout(Duration::from_secs(10))
        .expect("cancel was blocked behind receive")
        .is_err());
    assert_eq!(
        receiver
            .snapshot()
            .unwrap()
            .transfers
            .iter()
            .find(|t| t.transfer_id == 121)
            .unwrap()
            .status,
        "cancelled"
    );
    receiver.close();
    sender.close();
    let restored = session(&root.path().join("sender"));
    assert!(
        restored
            .snapshot()
            .unwrap()
            .events
            .iter()
            .any(|event| { event.transfer_id == Some(120) && event.kind == "share-stopped" }),
        "persisted activity is restored before the first snapshot"
    );
    restored.close();
}

#[test]
fn close_signals_active_receive_before_draining_calls_and_rejects_new_work() {
    let root = tempfile::tempdir().unwrap();
    let sender = session(&root.path().join("sender"));
    let receiver = session(&root.path().join("receiver"));
    let path = root.path().join("document.txt");
    std::fs::write(&path, b"content").unwrap();
    let share = sender
        .call(|core| core.share_files(crate::draft::sources(vec![path]).unwrap(), metadata(122)))
        .unwrap();
    let output = root.path().join("output");
    std::fs::create_dir_all(&output).unwrap();
    let receiving = receiver.clone();
    let (done, completion) = mpsc::channel();
    thread::spawn(move || {
        done.send(
            receiving
                .call(|core| core.receive(share.ticket, output.to_str().unwrap().into(), None)),
        )
        .unwrap()
    });
    wait_request(&sender, 122);
    let closing = receiver.clone();
    let (closed, closure) = mpsc::channel();
    thread::spawn(move || {
        closing.close();
        closed.send(()).unwrap();
    });
    closure
        .recv_timeout(Duration::from_secs(10))
        .expect("shutdown waited for receive before signalling it");
    assert!(completion
        .recv_timeout(Duration::from_secs(1))
        .unwrap()
        .is_err());
    assert!(matches!(receiver.snapshot(), Err("linux_session_closed")));
    assert!(receiver.changes().is_closed());
    sender.close();
}

#[test]
fn cancelling_before_preparation_registration_retains_intent_and_releases_worker() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let preparation = Preparation::new();
    preparation.request_cancel();
    let cancelling = preparation.clone();
    let target = session.clone();
    let (done, completion) = mpsc::channel();
    thread::spawn(move || {
        cancelling.cancel(&target);
        done.send(()).unwrap();
    });
    assert!(matches!(
        preparation.run(&session, vec![], metadata(0)),
        Err("progress_cancelled")
    ));
    completion
        .recv_timeout(Duration::from_secs(2))
        .expect("cancellation worker did not finish");
    assert!(session.snapshot().unwrap().transfers.is_empty());
    session.close();
}

#[test]
fn cancelling_after_preparation_completes_revokes_the_published_invitation() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let path = root.path().join("document.txt");
    std::fs::write(&path, b"content").unwrap();
    let preparation = Preparation::new();
    preparation
        .run(
            &session,
            crate::draft::sources(vec![path]).unwrap(),
            metadata(0),
        )
        .unwrap();
    preparation.cancel(&session);
    assert!(preparation.is_cancelled());
    let transfer = session.snapshot().unwrap().transfers.remove(0);
    assert!(!crate::presentation::is_active(&transfer));
    assert!(crate::presentation::invitation(&transfer).is_none());
    session.close();
}

fn wait_device(
    session: &Session,
    predicate: impl Fn(&crate::devices::Device) -> bool,
) -> crate::devices::Device {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        if let Some(device) = session
            .snapshot()
            .unwrap()
            .devices
            .list(now)
            .into_iter()
            .find(&predicate)
        {
            return device;
        }
        assert!(
            Instant::now() < deadline,
            "device consent did not reach the expected state"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_device_commands_require_mutual_consent_and_preserve_revocation() {
    use crate::devices::{self, DeviceAction, DeviceState};
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    };
    for revoke in [DeviceAction::Forget, DeviceAction::Block] {
        let root = tempfile::tempdir().unwrap();
        let sender = session(&root.path().join("sender"));
        let receiver = session(&root.path().join("receiver"));
        let source = root.path().join("pair.txt");
        std::fs::write(&source, b"explicit consent").unwrap();
        let output = root.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let share = sender
            .call(|core| {
                core.share_files(
                    vec![ShareSource {
                        kind: SourceKind::Path,
                        value: source.to_str().unwrap().into(),
                        display_name: None,
                        is_directory: false,
                    }],
                    metadata(220),
                )
            })
            .unwrap();
        let peer = receiver.clone();
        let worker = thread::spawn(move || {
            peer.call(|core| {
                core.receive(
                    share.ticket,
                    output.to_str().unwrap().into(),
                    Some("Other device".into()),
                )
            })
        });
        let request = wait_request(&sender, 220);
        sender
            .call(|core| core.respond_receiver_request(request.id, true, None))
            .unwrap();
        worker.join().unwrap().unwrap();
        let eligible = wait_device(&sender, |d| matches!(d.state, DeviceState::Eligible { .. }));
        sender
            .call(|core| {
                Ok(devices::execute(
                    core,
                    &eligible,
                    DeviceAction::Remember,
                    now(),
                ))
            })
            .unwrap()
            .unwrap();
        wait_device(&sender, |d| matches!(d.state, DeviceState::Outgoing { .. }));
        assert!(sender.snapshot().unwrap().devices.saved.is_empty());
        let incoming = wait_device(&receiver, |d| {
            matches!(d.state, DeviceState::Incoming { .. })
        });
        receiver
            .call(|core| {
                Ok(devices::execute(
                    core,
                    &incoming,
                    DeviceAction::Accept,
                    now(),
                ))
            })
            .unwrap()
            .unwrap();
        let saved = wait_device(&sender, |d| matches!(d.state, DeviceState::Saved { .. }));
        wait_device(&receiver, |d| matches!(d.state, DeviceState::Saved { .. }));
        assert_eq!(
            sender
                .call(|core| Ok(devices::execute(
                    core,
                    &eligible,
                    DeviceAction::Remember,
                    now()
                )))
                .unwrap(),
            Err("linux_device_changed")
        );
        sender
            .call(|core| {
                Ok(devices::execute(
                    core,
                    &saved,
                    DeviceAction::Label("  Desk laptop  ".into()),
                    now(),
                ))
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            sender.snapshot().unwrap().devices.saved[0]
                .local_label
                .as_deref(),
            Some("Desk laptop")
        );
        sender
            .call(|core| {
                Ok(devices::execute(
                    core,
                    &saved,
                    DeviceAction::Label("  ".into()),
                    now(),
                ))
            })
            .unwrap()
            .unwrap();
        assert_eq!(
            sender.snapshot().unwrap().devices.saved[0].local_label,
            None
        );
        let blocked = matches!(revoke, DeviceAction::Block);
        sender
            .call(|core| Ok(devices::execute(core, &saved, revoke, now())))
            .unwrap()
            .unwrap();
        assert!(sender.snapshot().unwrap().devices.saved.is_empty());
        if blocked {
            let denied = wait_device(&sender, |d| matches!(d.state, DeviceState::Blocked));
            sender
                .call(|core| {
                    Ok(devices::execute(
                        core,
                        &denied,
                        DeviceAction::Unblock,
                        now(),
                    ))
                })
                .unwrap()
                .unwrap();
            let state = sender.snapshot().unwrap().devices;
            assert!(state.blocked.is_empty());
            assert!(
                state.list(now()).is_empty(),
                "unblocked peers do not remain in device management"
            );
            assert!(
                state.saved.is_empty(),
                "unblocking must not restore consent"
            );
        }
        assert_eq!(
            sender
                .call(|core| Ok(devices::execute(
                    core,
                    &saved,
                    DeviceAction::Label("stale".into()),
                    now()
                )))
                .unwrap(),
            Err("linux_device_changed")
        );
        sender.close();
        receiver.close();
    }
}

#[test]
fn native_pairing_decline_never_creates_saved_access() {
    use crate::devices::{self, DeviceAction, DeviceState};
    let now = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    };
    for incoming in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let sender = session(&root.path().join("sender"));
        let receiver = session(&root.path().join("receiver"));
        let source = root.path().join("decline.txt");
        std::fs::write(&source, b"decline consent").unwrap();
        let output = root.path().join("output");
        std::fs::create_dir(&output).unwrap();
        let share = sender
            .call(|core| {
                core.share_files(
                    vec![ShareSource {
                        kind: SourceKind::Path,
                        value: source.to_str().unwrap().into(),
                        display_name: None,
                        is_directory: false,
                    }],
                    metadata(320),
                )
            })
            .unwrap();
        let peer = receiver.clone();
        let worker = thread::spawn(move || {
            peer.call(|core| {
                core.receive(
                    share.ticket,
                    output.to_str().unwrap().into(),
                    Some("Other device".into()),
                )
            })
        });
        let request = wait_request(&sender, 320);
        sender
            .call(|core| core.respond_receiver_request(request.id, true, None))
            .unwrap();
        worker.join().unwrap().unwrap();
        let eligible = wait_device(&sender, |d| matches!(d.state, DeviceState::Eligible { .. }));
        let (actor, target) = if incoming {
            sender
                .call(|core| {
                    Ok(devices::execute(
                        core,
                        &eligible,
                        DeviceAction::Remember,
                        now(),
                    ))
                })
                .unwrap()
                .unwrap();
            let pending = wait_device(&receiver, |d| {
                matches!(d.state, DeviceState::Incoming { .. })
            });
            (&receiver, pending)
        } else {
            (&sender, eligible)
        };
        actor
            .call(|core| {
                Ok(devices::execute(
                    core,
                    &target,
                    DeviceAction::Decline,
                    now(),
                ))
            })
            .unwrap()
            .unwrap();
        assert!(sender.snapshot().unwrap().devices.saved.is_empty());
        assert!(receiver.snapshot().unwrap().devices.saved.is_empty());
        assert_eq!(
            actor
                .call(|core| Ok(devices::execute(
                    core,
                    &target,
                    if incoming {
                        DeviceAction::Accept
                    } else {
                        DeviceAction::Remember
                    },
                    now()
                )))
                .unwrap(),
            Err("linux_device_changed")
        );
        sender.close();
        receiver.close();
    }
}

#[path = "targeted_tests.rs"]
mod targeted_tests;

#[path = "settings_tests.rs"]
mod settings_tests;
