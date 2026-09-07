use super::*;
use crate::reconfiguration::Maintenance;
use crate::{preferences::Preferences, reconfiguration::apply_with};

#[test]
fn reconfiguration_restores_identity_and_preferences_after_failure() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let old = session(&profile);
    let identity = old.call(|core| Ok(core.status().endpoint_id)).unwrap();
    let mut previous = Preferences::defaults(root.path().join("downloads"), "Native".into());
    previous.network.mode = CoreRelayMode::LocalOnly;
    previous.save(&profile).unwrap();
    let mut next = previous.clone();
    next.network.mode = CoreRelayMode::StrictCustom;
    let result = apply_with(
        old,
        &profile,
        previous.clone(),
        next,
        Maintenance::None,
        |prefs| {
            if prefs.network.mode == CoreRelayMode::StrictCustom {
                Err("error_initialization")
            } else {
                Ok(session(&profile))
            }
        },
    );
    assert_eq!(result.error, Some("relay_apply_failed"));
    let restored = result.session.unwrap();
    assert_eq!(
        restored.call(|core| Ok(core.status().endpoint_id)).unwrap(),
        identity
    );
    assert_eq!(
        Preferences::load(&profile, previous.clone())
            .unwrap()
            .network,
        previous.network
    );
    let result = apply_with(
        restored,
        &profile,
        previous.clone(),
        previous,
        Maintenance::Cache,
        |_| Ok(session(&profile)),
    );
    assert_eq!(result.error, None);
    let restarted = result.session.unwrap();
    assert_eq!(
        restarted
            .call(|core| Ok(core.status().endpoint_id))
            .unwrap(),
        identity
    );
    restarted.close();
}

#[test]
fn reconfiguration_refuses_active_shares_without_closing_session() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let old = session(&profile);
    let source = root.path().join("source");
    std::fs::write(&source, b"keep sharing").unwrap();
    old.call(|core| core.share_files(crate::draft::sources(vec![source]).unwrap(), metadata(950)))
        .unwrap();
    let prefs = Preferences::defaults(root.path().join("downloads"), "Native".into());
    let result = apply_with(
        old.clone(),
        &profile,
        prefs.clone(),
        prefs,
        Maintenance::Cache,
        |_| panic!("must not reopen while sharing"),
    );
    assert_eq!(result.error, Some("relay_apply_active_transfers"));
    assert!(Arc::ptr_eq(&old, &result.session.unwrap()));
    assert!(old.snapshot().unwrap().has_obligations());
    old.close();
}

#[test]
fn failed_restart_and_failed_restore_leave_a_recoverable_offline_result() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join("profile");
    let old = session(&profile);
    let prefs = Preferences::defaults(root.path().join("downloads"), "Native".into());
    let result = apply_with(
        old,
        &profile,
        prefs.clone(),
        prefs,
        Maintenance::None,
        |_| Err("error_initialization"),
    );
    assert!(result.session.is_none());
    assert_eq!(result.error, Some("relay_restore_failed"));
    let recovered = session(&profile);
    assert!(recovered.snapshot().is_ok());
    recovered.close();
}
