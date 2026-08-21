//! Persistence open: one SQLite pool, every domain schema, [`AppDataStores`].
//!
//! Runtime talks to domain stores — not a raw pool. Schema application for each
//! domain is owned here (not orchestrated from the invitation store).

use std::{path::Path, str::FromStr};

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::{
    blocked_devices::{self, BlockStore},
    device_relationship::DeviceRelationshipStore,
    identity_recovery::IdentityRecoveryStore,
    invitation::Repository,
    pairing_eligibility::PairingEligibilityStore,
    secure_secret::{self, SecretMetadataStore},
    targeted_transfer::{self, TargetedTransferStore},
};

/// Concrete domain stores for one app-data profile.
#[derive(Clone)]
pub(crate) struct AppDataStores {
    /// Invitation-transfer history and related invitation tables.
    pub(crate) invitation: Repository,
    /// Targeted-transfer durable rows.
    pub(crate) targeted: TargetedTransferStore,
    /// Mutual-consent device relationships (+ generation tombstones).
    pub(crate) relationships: DeviceRelationshipStore,
    /// Post-transfer pairing eligibility rows.
    pub(crate) eligibility: PairingEligibilityStore,
    /// Non-secret metadata for protected credential handles.
    pub(crate) secrets: SecretMetadataStore,
    /// Identity-wide deny list.
    pub(crate) blocked: BlockStore,
    /// Explicit endpoint-identity reset transaction.
    pub(crate) identity_recovery: IdentityRecoveryStore,
}

/// Create the profile pool, apply all domain schemas, return [`AppDataStores`].
pub(crate) async fn open_all(app_data_dir: &Path) -> Result<AppDataStores> {
    let db_path = app_data_dir.join("vnidrop.sqlite3");
    let options = SqliteConnectOptions::from_str("sqlite://")?
        .filename(db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .context("failed to open app data sqlite")?;

    // Unreleased device-history prototype tables — no migration path.
    for table in ["held_offers", "grants_held", "grants_issued", "contacts"] {
        sqlx::query(&format!("DROP TABLE IF EXISTS {table}"))
            .execute(&pool)
            .await?;
    }

    let invitation = Repository::from_pool(pool.clone());
    invitation.ensure_schema().await?;
    blocked_devices::ensure_schema(&pool).await?;
    secure_secret::ensure_schema(&pool).await?;
    DeviceRelationshipStore::ensure_schema(&pool).await?;
    targeted_transfer::ensure_schema(&pool).await?;
    PairingEligibilityStore::ensure_schema(&pool).await?;

    Ok(AppDataStores {
        targeted: TargetedTransferStore::new(pool.clone()),
        relationships: DeviceRelationshipStore::new(pool.clone()),
        eligibility: PairingEligibilityStore::new(pool.clone()),
        secrets: SecretMetadataStore::new(pool.clone()),
        identity_recovery: IdentityRecoveryStore::new(pool.clone()),
        blocked: BlockStore::new(pool),
        invitation,
    })
}
