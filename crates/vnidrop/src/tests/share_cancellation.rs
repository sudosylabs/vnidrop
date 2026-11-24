use std::{sync::Arc, time::Duration};

use crate::{
    secure_secret::{install_platform_secret_store_for_test, FaultInjectingSecretStore},
    CoreEvent, CoreEventSink, CoreLimits, CoreNetworkConfig, CoreRelayMode, ShareMetadataInput,
    ShareSource, SourceKind, TransferAccessMode, VnidropCore, VnidropError,
};

struct Sink;
impl CoreEventSink for Sink {
    fn on_event(&self, _: CoreEvent) {}
}

#[test]
fn cancelling_after_share_commit_removes_availability_and_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let profile = std::fs::canonicalize(directory.path()).unwrap();
    install_platform_secret_store_for_test(
        &profile,
        Arc::new(FaultInjectingSecretStore::default()),
    );
    let source = profile.join("payload.txt");
    std::fs::write(&source, b"cancel after commit").unwrap();
    let open = || {
        VnidropCore::initialize_with_limits_and_network_config(
            profile.to_string_lossy().into_owned(),
            Arc::new(Sink),
            CoreLimits::default(),
            CoreNetworkConfig {
                mode: CoreRelayMode::LocalOnly,
                relay_urls: vec![],
            },
        )
        .unwrap()
    };
    let core = open();
    let (arrived, _release) = core.hold_share_publication_for_test();
    let sharing = core.clone();
    let worker = std::thread::spawn(move || {
        sharing.share_files(
            vec![ShareSource {
                kind: SourceKind::Path,
                value: source.to_string_lossy().into_owned(),
                display_name: None,
                is_directory: false,
            }],
            ShareMetadataInput {
                transfer_id: 712,
                transfer_name: Some("Cancel".into()),
                sender_name: None,
                access_mode: TransferAccessMode::Public,
            },
        )
    });
    arrived.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(core.list_transfers().unwrap()[0].status, "sharing");
    let cancelling = core.clone();
    let (completed, waiting) = std::sync::mpsc::sync_channel(1);
    let cancel_worker =
        std::thread::spawn(move || completed.send(cancelling.cancel_transfer(712)).unwrap());
    waiting
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    cancel_worker.join().unwrap();
    assert!(matches!(
        worker.join().unwrap(),
        Err(VnidropError::Cancelled { .. })
    ));
    assert_eq!(core.list_transfers().unwrap()[0].status, "stopped");
    assert_eq!(
        core.runtime_obligation_facts()
            .unwrap()
            .invitation_provider_availability,
        0
    );
    assert!(core
        .list_events(None)
        .unwrap()
        .iter()
        .any(|event| event.transfer_id == Some(712) && event.kind == "cancelled"));
    core.shutdown();
    drop(core);
    let reopened = open();
    assert_eq!(reopened.list_transfers().unwrap()[0].status, "stopped");
    assert_eq!(
        reopened
            .runtime_obligation_facts()
            .unwrap()
            .invitation_provider_availability,
        0
    );
    reopened.shutdown();
}

#[cfg(unix)]
#[test]
fn failed_source_finishes_import_while_a_sibling_reader_is_stalled() {
    use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};

    let directory = tempfile::tempdir().unwrap();
    let profile = std::fs::canonicalize(directory.path()).unwrap();
    install_platform_secret_store_for_test(
        &profile,
        Arc::new(FaultInjectingSecretStore::default()),
    );
    let core = VnidropCore::initialize_with_limits_and_network_config(
        profile.to_string_lossy().into_owned(),
        Arc::new(Sink),
        CoreLimits::default(),
        CoreNetworkConfig {
            mode: CoreRelayMode::LocalOnly,
            relay_urls: vec![],
        },
    )
    .unwrap();
    let write_only = std::fs::File::create(profile.join("write-only")).unwrap();
    let fifo = profile.join("stalled");
    let path = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    let stalled = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&fifo)
        .unwrap();
    let sharing = core.clone();
    let (done, outcome) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        let sources = vec![
            ShareSource {
                kind: SourceKind::FileDescriptor,
                value: write_only.as_raw_fd().to_string(),
                display_name: Some("failed".into()),
                is_directory: false,
            },
            ShareSource {
                kind: SourceKind::Path,
                value: fifo.to_string_lossy().into_owned(),
                display_name: Some("stalled".into()),
                is_directory: false,
            },
        ];
        done.send(sharing.share_files(
            sources,
            ShareMetadataInput {
                transfer_id: 713,
                transfer_name: None,
                sender_name: None,
                access_mode: TransferAccessMode::Public,
            },
        ))
        .unwrap();
    });

    let result = outcome.recv_timeout(Duration::from_secs(5));
    drop(stalled);
    worker.join().unwrap();
    assert!(result
        .expect("import waited for an unrelated reader after a source failed")
        .is_err());
    assert_eq!(core.list_transfers().unwrap()[0].status, "failed");
    core.shutdown();
}
