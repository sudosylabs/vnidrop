use crate::{error::Result, preferences::Preferences, session::Session};
use std::{path::Path, sync::Arc};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Maintenance {
    None,
    Cache,
    Temporary,
}

pub struct Reconfigured {
    pub session: Option<Arc<Session>>,
    pub preferences: Preferences,
    pub error: Option<&'static str>,
}

pub fn apply(
    old: Arc<Session>,
    profile: &Path,
    previous: Preferences,
    next: Preferences,
    maintenance: Maintenance,
) -> Reconfigured {
    apply_with(old, profile, previous, next, maintenance, |preferences| {
        Session::open(
            profile.to_str().ok_or("error_filesystem")?.into(),
            preferences.network_config(),
        )
    })
}

pub(crate) fn apply_with(
    old: Arc<Session>,
    profile: &Path,
    previous: Preferences,
    next: Preferences,
    maintenance: Maintenance,
    open: impl Fn(&Preferences) -> Result<Arc<Session>>,
) -> Reconfigured {
    match old.snapshot() {
        Ok(snapshot) if !snapshot.has_obligations() => {}
        Ok(_) => {
            return Reconfigured {
                session: Some(old),
                preferences: previous,
                error: Some("relay_apply_active_transfers"),
            }
        }
        Err(key) => {
            return Reconfigured {
                session: Some(old),
                preferences: previous,
                error: Some(key),
            }
        }
    }
    old.close();
    let operation = if maintenance == Maintenance::Cache {
        vnidrop::clear_inactive_transfer_cache(profile.to_string_lossy().into_owned())
            .map(|_| ())
            .map_err(crate::error::message_key)
    } else if maintenance == Maintenance::Temporary {
        crate::storage::cleanup(&next.receive_directory).map(|_| ())
    } else {
        Ok(())
    };
    let changed = operation.and_then(|()| {
        let session = open(&next)?;
        if let Err(key) = next.save(profile) {
            session.close();
            return Err(key);
        }
        Ok(session)
    });
    match changed {
        Ok(session) => Reconfigured {
            session: Some(session),
            preferences: next,
            error: None,
        },
        Err(_) => match open(&previous) {
            Ok(session) => Reconfigured {
                session: Some(session),
                preferences: previous,
                error: Some("relay_apply_failed"),
            },
            Err(_) => Reconfigured {
                session: None,
                preferences: previous,
                error: Some("relay_restore_failed"),
            },
        },
    }
}
