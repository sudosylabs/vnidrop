//! Storage for device history: contacts, the grants that make them usable, and
//! the block list.
//!
//! Split out of [`crate::repository`] to keep that file focused; the tables are
//! created as part of the same schema migration and share its pool.
//!
//! Grant secrets live here. They are key material and follow the same rule as
//! tickets: never logged, never emitted in an event, never returned across the
//! UniFFI boundary.

use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool};

use crate::grant::{parse_secret, GrantId, HeldGrant, IssuedGrant};

/// How long a dead grant is kept before being swept.
///
/// A revoked grant stays as a tombstone so a returning peer is told `Revoked`
/// rather than `Unknown`; after this long, a peer that has not come back is
/// unlikely to, and the row is noise.
pub(crate) const DEAD_GRANT_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// A device the user has transferred with and chosen to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Contact {
    pub(crate) endpoint_id: String,
    /// Set by the local user. Never overwritten by a name the remote claims.
    pub(crate) local_label: Option<String>,
    /// Last name the remote sent. Untrusted display data.
    pub(crate) remote_display_name: Option<String>,
    /// Encoded `EndpointAddr` from the last successful connection, so the peer
    /// stays dialable in relay profiles without public address lookup.
    pub(crate) last_known_addr: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) last_transfer_at: Option<i64>,
}

pub(crate) async fn ensure_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS contacts (
            endpoint_id TEXT PRIMARY KEY,
            local_label TEXT,
            remote_display_name TEXT,
            last_known_addr TEXT,
            created_at INTEGER NOT NULL,
            last_transfer_at INTEGER
        );
        "#,
    )
    .execute(pool)
    .await?;

    // Authoritative side: only the issuer can validate or revoke these.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS grants_issued (
            grant_id TEXT PRIMARY KEY,
            grant_secret TEXT NOT NULL,
            issued_to_endpoint_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            revoked_at INTEGER
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_grants_issued_endpoint ON grants_issued(issued_to_endpoint_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS grants_held (
            grant_id TEXT PRIMARY KEY,
            grant_secret TEXT NOT NULL,
            peer_endpoint_id TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_grants_held_endpoint ON grants_held(peer_endpoint_id);",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS held_offers (
            offer_id TEXT PRIMARY KEY,
            endpoint_id TEXT NOT NULL,
            transfer_id INTEGER NOT NULL,
            ticket TEXT NOT NULL,
            transfer_name TEXT NOT NULL,
            sender_display_name TEXT,
            file_count INTEGER NOT NULL,
            total_bytes INTEGER NOT NULL,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_held_offers_endpoint ON held_offers(endpoint_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS blocked_endpoints (
            endpoint_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// An offer that could not be delivered because the target was not running.
///
/// Held on this device, not a server: the share stays here and the receiver
/// collects the ticket when its app next comes to the foreground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldOffer {
    pub(crate) offer_id: String,
    pub(crate) endpoint_id: String,
    pub(crate) transfer_id: u64,
    pub(crate) ticket: String,
    pub(crate) transfer_name: String,
    pub(crate) sender_display_name: Option<String>,
    pub(crate) file_count: u64,
    pub(crate) total_bytes: u64,
    pub(crate) created_at: i64,
}

/// Contacts, grants, and blocks over the shared repository pool.
#[derive(Debug, Clone)]
pub(crate) struct ContactStore {
    pool: SqlitePool,
}

impl ContactStore {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // -- contacts ---------------------------------------------------------

    /// Record a contact, or refresh the untrusted display name of an existing
    /// one. The local label is deliberately left untouched.
    pub(crate) async fn upsert_contact(
        &self,
        endpoint_id: &str,
        remote_display_name: Option<&str>,
        now_ms: i64,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO contacts (endpoint_id, remote_display_name, created_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(endpoint_id) DO UPDATE SET
                remote_display_name = COALESCE(excluded.remote_display_name, contacts.remote_display_name)
            "#,
        )
        .bind(endpoint_id)
        .bind(remote_display_name)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn set_contact_label(
        &self,
        endpoint_id: &str,
        label: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE contacts SET local_label = ?2 WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .bind(label)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn touch_transfer(&self, endpoint_id: &str, now_ms: i64) -> Result<()> {
        sqlx::query("UPDATE contacts SET last_transfer_at = ?2 WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .bind(now_ms)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_last_known_addr(&self, endpoint_id: &str, addr: &str) -> Result<()> {
        sqlx::query("UPDATE contacts SET last_known_addr = ?2 WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .bind(addr)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn list_contacts(&self) -> Result<Vec<Contact>> {
        let rows = sqlx::query(
            r#"
            SELECT endpoint_id, local_label, remote_display_name, last_known_addr,
                   created_at, last_transfer_at
            FROM contacts
            ORDER BY COALESCE(last_transfer_at, created_at) DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| Contact {
                endpoint_id: row.get(0),
                local_label: row.get(1),
                remote_display_name: row.get(2),
                last_known_addr: row.get(3),
                created_at: row.get(4),
                last_transfer_at: row.get(5),
            })
            .collect())
    }

    pub(crate) async fn find_contact(&self, endpoint_id: &str) -> Result<Option<Contact>> {
        Ok(self
            .list_contacts()
            .await?
            .into_iter()
            .find(|contact| contact.endpoint_id == endpoint_id))
    }

    /// Remove a contact and every grant in both directions.
    ///
    /// Returns the ids of the grants this device had issued, so the caller can
    /// send the best-effort revoke notification. Deletion succeeds regardless of
    /// whether that notification is ever delivered.
    pub(crate) async fn delete_contact(&self, endpoint_id: &str) -> Result<Vec<GrantId>> {
        let issued = self.issued_grant_ids_for(endpoint_id).await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM grants_issued WHERE issued_to_endpoint_id = ?1")
            .bind(endpoint_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM grants_held WHERE peer_endpoint_id = ?1")
            .bind(endpoint_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM contacts WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(issued)
    }

    /// Wholesale delete, for the same surface that clears transfer history.
    pub(crate) async fn delete_all_contacts(&self) -> Result<Vec<GrantId>> {
        let issued = self.all_issued_grant_ids().await?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM grants_issued")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM grants_held")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM contacts")
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(issued)
    }

    // -- issued grants ----------------------------------------------------

    pub(crate) async fn insert_issued_grant(&self, grant: &IssuedGrant) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO grants_issued
                (grant_id, grant_secret, issued_to_endpoint_id, created_at, expires_at, revoked_at)
            VALUES (?1, ?2, ?3, ?4, ?5, NULL)
            "#,
        )
        .bind(grant.grant_id.encode())
        .bind(grant.secret.encode())
        .bind(&grant.issued_to_endpoint_id)
        .bind(grant.created_at)
        .bind(grant.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Look up a grant by the id a peer presented.
    ///
    /// A row whose secret fails to parse is corrupt storage, not a usable
    /// grant: surface the error rather than silently refusing the peer, which
    /// would look like revocation.
    pub(crate) async fn find_issued_grant(&self, grant_id: GrantId) -> Result<Option<IssuedGrant>> {
        let row = sqlx::query(
            r#"
            SELECT grant_id, grant_secret, issued_to_endpoint_id, created_at, expires_at, revoked_at
            FROM grants_issued
            WHERE grant_id = ?1
            "#,
        )
        .bind(grant_id.encode())
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_issued_grant).transpose()
    }

    /// Push the idle deadline forward after an accepted proof.
    pub(crate) async fn renew_issued_grant(
        &self,
        grant_id: GrantId,
        expires_at: Option<i64>,
    ) -> Result<()> {
        sqlx::query("UPDATE grants_issued SET expires_at = ?2 WHERE grant_id = ?1")
            .bind(grant_id.encode())
            .bind(expires_at)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// End the relationship from the issuing side. Tombstoned rather than
    /// deleted so a later attempt is answered `Revoked` instead of `Unknown`.
    pub(crate) async fn revoke_issued_grant(&self, grant_id: GrantId, now_ms: i64) -> Result<()> {
        sqlx::query(
            "UPDATE grants_issued SET revoked_at = ?2 WHERE grant_id = ?1 AND revoked_at IS NULL",
        )
        .bind(grant_id.encode())
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn revoke_issued_grants_for(
        &self,
        endpoint_id: &str,
        now_ms: i64,
    ) -> Result<Vec<GrantId>> {
        let ids = self.issued_grant_ids_for(endpoint_id).await?;
        sqlx::query(
            "UPDATE grants_issued SET revoked_at = ?2 WHERE issued_to_endpoint_id = ?1 AND revoked_at IS NULL",
        )
        .bind(endpoint_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(ids)
    }

    async fn issued_grant_ids_for(&self, endpoint_id: &str) -> Result<Vec<GrantId>> {
        let rows =
            sqlx::query("SELECT grant_id FROM grants_issued WHERE issued_to_endpoint_id = ?1")
                .bind(endpoint_id)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter()
            .map(|row| GrantId::decode(row.get::<String, _>(0).as_str()))
            .collect()
    }

    async fn all_issued_grant_ids(&self) -> Result<Vec<GrantId>> {
        let rows = sqlx::query("SELECT grant_id FROM grants_issued")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| GrantId::decode(row.get::<String, _>(0).as_str()))
            .collect()
    }

    // -- held grants ------------------------------------------------------

    pub(crate) async fn insert_held_grant(&self, grant: &HeldGrant) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO grants_held
                (grant_id, grant_secret, peer_endpoint_id, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(grant_id) DO UPDATE SET
                grant_secret = excluded.grant_secret,
                expires_at = excluded.expires_at
            "#,
        )
        .bind(grant.grant_id.encode())
        .bind(grant.secret.encode())
        .bind(&grant.peer_endpoint_id)
        .bind(grant.created_at)
        .bind(grant.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The capability to reach `peer_endpoint_id`, if this device holds one.
    ///
    /// Newest wins: re-pairing issues a fresh grant, and the old one is dead on
    /// the issuer's side anyway.
    pub(crate) async fn held_grant_for(&self, peer_endpoint_id: &str) -> Result<Option<HeldGrant>> {
        let row = sqlx::query(
            r#"
            SELECT grant_id, grant_secret, peer_endpoint_id, created_at, expires_at
            FROM grants_held
            WHERE peer_endpoint_id = ?1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(peer_endpoint_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_held_grant).transpose()
    }

    /// Drop a grant this device holds, after the issuer reported it dead.
    pub(crate) async fn delete_held_grant(&self, grant_id: GrantId) -> Result<()> {
        sqlx::query("DELETE FROM grants_held WHERE grant_id = ?1")
            .bind(grant_id.encode())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- block list -------------------------------------------------------

    /// Block an endpoint and revoke anything it still holds, so blocking is not
    /// merely cosmetic while a live grant remains.
    pub(crate) async fn block_endpoint(&self, endpoint_id: &str, now_ms: i64) -> Result<()> {
        self.revoke_issued_grants_for(endpoint_id, now_ms).await?;
        sqlx::query(
            "INSERT INTO blocked_endpoints (endpoint_id, created_at) VALUES (?1, ?2)
             ON CONFLICT(endpoint_id) DO NOTHING",
        )
        .bind(endpoint_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn unblock_endpoint(&self, endpoint_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM blocked_endpoints WHERE endpoint_id = ?1")
            .bind(endpoint_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub(crate) async fn is_blocked(&self, endpoint_id: &str) -> Result<bool> {
        let row =
            sqlx::query("SELECT EXISTS(SELECT 1 FROM blocked_endpoints WHERE endpoint_id = ?1)")
                .bind(endpoint_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.get::<i64, _>(0) == 1)
    }

    pub(crate) async fn list_blocked(&self) -> Result<Vec<String>> {
        let rows =
            sqlx::query("SELECT endpoint_id FROM blocked_endpoints ORDER BY created_at DESC")
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    // -- held offers ------------------------------------------------------

    pub(crate) async fn insert_held_offer(&self, offer: &HeldOffer) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO held_offers
                (offer_id, endpoint_id, transfer_id, ticket, transfer_name,
                 sender_display_name, file_count, total_bytes, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
        )
        .bind(&offer.offer_id)
        .bind(&offer.endpoint_id)
        .bind(offer.transfer_id as i64)
        .bind(&offer.ticket)
        .bind(&offer.transfer_name)
        .bind(offer.sender_display_name.as_deref())
        .bind(offer.file_count as i64)
        .bind(offer.total_bytes as i64)
        .bind(offer.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Offers waiting for one device to come and collect them.
    pub(crate) async fn held_offers_for(&self, endpoint_id: &str) -> Result<Vec<HeldOffer>> {
        let rows = sqlx::query(
            r#"
            SELECT offer_id, endpoint_id, transfer_id, ticket, transfer_name,
                   sender_display_name, file_count, total_bytes, created_at
            FROM held_offers
            WHERE endpoint_id = ?1
            ORDER BY created_at ASC
            "#,
        )
        .bind(endpoint_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_held_offer).collect())
    }

    pub(crate) async fn list_held_offers(&self) -> Result<Vec<HeldOffer>> {
        let rows = sqlx::query(
            r#"
            SELECT offer_id, endpoint_id, transfer_id, ticket, transfer_name,
                   sender_display_name, file_count, total_bytes, created_at
            FROM held_offers
            ORDER BY created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_held_offer).collect())
    }

    /// Consumed once handed over, so a device polling twice is not offered the
    /// same transfer again.
    pub(crate) async fn delete_held_offers(&self, offer_ids: &[String]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for offer_id in offer_ids {
            sqlx::query("DELETE FROM held_offers WHERE offer_id = ?1")
                .bind(offer_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn delete_held_offers_for_transfer(&self, transfer_id: u64) -> Result<()> {
        sqlx::query("DELETE FROM held_offers WHERE transfer_id = ?1")
            .bind(transfer_id as i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // -- maintenance ------------------------------------------------------

    #[cfg(test)]
    pub(crate) async fn corrupt_secret_for_test(&self, grant_id: GrantId) -> Result<()> {
        sqlx::query("UPDATE grants_issued SET grant_secret = 'not-hex' WHERE grant_id = ?1")
            .bind(grant_id.encode())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Drop grants that lapsed or were revoked long enough ago that no peer
    /// still needs to be told. Keeps tombstones bounded.
    pub(crate) async fn purge_dead_grants(&self, before_ms: i64) -> Result<u64> {
        let issued = sqlx::query(
            "DELETE FROM grants_issued
             WHERE (expires_at IS NOT NULL AND expires_at < ?1)
                OR (revoked_at IS NOT NULL AND revoked_at < ?1)",
        )
        .bind(before_ms)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(issued)
    }
}

fn row_to_held_offer(row: sqlx::sqlite::SqliteRow) -> HeldOffer {
    HeldOffer {
        offer_id: row.get(0),
        endpoint_id: row.get(1),
        transfer_id: row.get::<i64, _>(2) as u64,
        ticket: row.get(3),
        transfer_name: row.get(4),
        sender_display_name: row.get(5),
        file_count: row.get::<i64, _>(6) as u64,
        total_bytes: row.get::<i64, _>(7) as u64,
        created_at: row.get(8),
    }
}

fn row_to_issued_grant(row: sqlx::sqlite::SqliteRow) -> Result<IssuedGrant> {
    let grant_id = GrantId::decode(row.get::<String, _>(0).as_str())?;
    let secret = parse_secret(row.get::<String, _>(1).as_str())
        .context("stored grant secret is unusable")?;
    Ok(IssuedGrant {
        grant_id,
        secret,
        issued_to_endpoint_id: row.get(2),
        created_at: row.get(3),
        expires_at: row.get(4),
        revoked_at: row.get(5),
    })
}

fn row_to_held_grant(row: sqlx::sqlite::SqliteRow) -> Result<HeldGrant> {
    let grant_id = GrantId::decode(row.get::<String, _>(0).as_str())?;
    let secret = parse_secret(row.get::<String, _>(1).as_str())
        .context("stored grant secret is unusable")?;
    Ok(HeldGrant {
        grant_id,
        secret,
        peer_endpoint_id: row.get(2),
        created_at: row.get(3),
        expires_at: row.get(4),
    })
}
