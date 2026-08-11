//! Persistence open: one SQLite pool, every domain schema, [`AppDataStores`].
//!
//! Runtime talks to domain stores — not a raw pool. Unmigrated modules may still
//! take [`AppDataStores::pool_for_unmigrated`] until their own stores deepen.

use std::{path::Path, str::FromStr};

use anyhow::{Context, Result};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

use crate::{
    blocked_devices::BlockStore, repository::Repository, targeted_transfer::TargetedTransferStore,
};

/// Concrete domain stores for one app-data profile.
#[derive(Clone)]
pub(crate) struct AppDataStores {
    /// Invitation-transfer history and related invitation tables.
    pub(crate) invitation: Repository,
    /// Targeted-transfer durable rows.
    pub(crate) targeted: TargetedTransferStore,
    /// Identity-wide deny list.
    pub(crate) blocked: BlockStore,
    pool: SqlitePool,
}

impl AppDataStores {
    /// Temporary: remaining domain modules still construct on a shared pool.
    /// Do not add new callers — migrate them to domain stores instead.
    pub(crate) fn pool_for_unmigrated(&self) -> SqlitePool {
        self.pool.clone()
    }
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

    // Invitation ensure_schema still orchestrates cross-domain schemas until
    // each domain store owns its ensure_schema call from this path alone.
    let invitation = Repository::from_pool(pool.clone());
    invitation.ensure_schema().await?;

    Ok(AppDataStores {
        targeted: TargetedTransferStore::new(pool.clone()),
        blocked: BlockStore::new(pool.clone()),
        invitation,
        pool,
    })
}
