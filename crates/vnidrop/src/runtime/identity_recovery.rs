//! Pre-start recovery for a missing or corrupted protected endpoint identity.

use std::{path::PathBuf, sync::Arc};

use super::{IdentityMode, VnidropCore};
use crate::{
    api::{CoreEventSink, CoreLimits, CoreNetworkConfig},
    error::VnidropError,
    secure_secret::{lock_profile, platform_secret_store, SecretCustody, SecureSecretStore},
};

impl VnidropCore {
    pub(super) fn reset_unrecoverable_identity_protected(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        network_config: CoreNetworkConfig,
    ) -> Result<Arc<Self>, VnidropError> {
        let app_data_path = PathBuf::from(app_data_dir);
        std::fs::create_dir_all(&app_data_path).map_err(VnidropError::filesystem)?;
        let app_data_path =
            std::fs::canonicalize(app_data_path).map_err(VnidropError::filesystem)?;
        let profile_lock = lock_profile(&app_data_path)?;
        let store = platform_secret_store(&app_data_path)?;
        Self::reset_unrecoverable_identity_with_store(
            app_data_path,
            event_sink,
            limits,
            network_config,
            store,
            profile_lock,
        )
    }

    fn reset_unrecoverable_identity_with_store(
        app_data_path: PathBuf,
        event_sink: Arc<dyn CoreEventSink>,
        limits: CoreLimits,
        network_config: CoreNetworkConfig,
        store: Arc<dyn SecureSecretStore>,
        profile_lock: crate::secure_secret::ProfileLock,
    ) -> Result<Arc<Self>, VnidropError> {
        let recovery_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        recovery_runtime.block_on(async {
            let stores = crate::persistence::open_all(&app_data_path)
                .await
                .map_err(VnidropError::repository)?;
            let custody =
                SecretCustody::for_explicit_identity_reset(stores.secrets.clone(), store.clone());
            custody.require_unrecoverable_endpoint_identity().await?;
            let handles = stores
                .identity_recovery
                .reset_identity_bound_state()
                .await?;
            custody.delete_reset_handles(handles).await
        })?;
        Self::initialize_with_identity_mode(
            app_data_path.to_string_lossy().into_owned(),
            event_sink,
            limits,
            network_config,
            IdentityMode::Protected {
                store,
                profile_lock,
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn reset_unrecoverable_identity_with_test_secret_store(
        app_data_dir: String,
        event_sink: Arc<dyn CoreEventSink>,
        store: Arc<dyn SecureSecretStore>,
    ) -> Result<Arc<Self>, VnidropError> {
        let app_data_path = PathBuf::from(&app_data_dir);
        std::fs::create_dir_all(&app_data_path).map_err(VnidropError::filesystem)?;
        let profile_lock = crate::secure_secret::unlocked_profile_for_test(&app_data_path)?;
        Self::reset_unrecoverable_identity_with_store(
            app_data_path,
            event_sink,
            CoreLimits::default(),
            CoreNetworkConfig::default(),
            store,
            profile_lock,
        )
    }
}
