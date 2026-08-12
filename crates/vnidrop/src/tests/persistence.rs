//! Persistence open returns domain stores without exporting a raw pool to callers.

use sqlx::Row;

use crate::persistence;

async fn open_profile_pool(app_data_dir: &std::path::Path) -> sqlx::SqlitePool {
    let db = app_data_dir.join("vnidrop.sqlite3");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db)
        .create_if_missing(false);
    sqlx::SqlitePool::connect_with(options).await.unwrap()
}

#[tokio::test]
async fn open_all_returns_all_domain_stores_and_schemas() {
    let temp = tempfile::tempdir().unwrap();
    let stores = persistence::open_all(temp.path()).await.unwrap();

    assert!(stores.blocked.list_blocked().await.unwrap().is_empty());
    assert!(stores.targeted.list().await.unwrap().is_empty());
    assert!(stores.invitation.list_transfers().await.unwrap().is_empty());
    assert!(stores
        .eligibility
        .list_summaries()
        .await
        .unwrap()
        .is_empty());

    let pool = open_profile_pool(temp.path()).await;
    for table in [
        "device_relationships",
        "relationship_generation_tombstones",
        "pairing_eligibilities",
        "protected_secret_refs",
        "blocked_endpoints",
        "targeted_transfers",
        "targeted_completion_outbox",
        "targeted_payload_release_outbox",
        "transfers",
    ] {
        let row = sqlx::query(&format!(
            "SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = '{table}'"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row.get::<i64, _>("n"),
            1,
            "{table} must exist after open_all"
        );
    }
}

#[tokio::test]
async fn open_all_migrates_targeted_completion_retry_schedule() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("vnidrop.sqlite3");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE targeted_completion_outbox (
            transfer_id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    drop(pool);

    let _stores = persistence::open_all(temp.path()).await.unwrap();
    let pool = open_profile_pool(temp.path()).await;
    let columns = sqlx::query("PRAGMA table_info(targeted_completion_outbox)")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "next_attempt_at"));
}

#[tokio::test]
async fn open_all_migrates_name_columns_without_losing_existing_local_labels() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("vnidrop.sqlite3");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&db)
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
    sqlx::query(
        r#"
        CREATE TABLE device_relationships (
            remote_endpoint_id TEXT PRIMARY KEY, state TEXT NOT NULL,
            generation INTEGER NOT NULL, minimum_protocol_version INTEGER NOT NULL,
            session_id TEXT, issued_grant_handle TEXT, held_grant_handle TEXT,
            issued_grant_id TEXT, held_grant_id TEXT, peer_ack INTEGER NOT NULL,
            local_ack INTEGER NOT NULL, created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL, local_label TEXT
        )
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO device_relationships (
            remote_endpoint_id, state, generation, minimum_protocol_version,
            peer_ack, local_ack, created_at, updated_at, local_label
        ) VALUES ('peer', 'saved', 1, 1, 1, 1, 10, 20, 'My tablet')
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    drop(pool);

    let stores = persistence::open_all(temp.path()).await.unwrap();
    let saved = stores.relationships.list_saved_devices().await.unwrap();
    assert_eq!(saved[0].local_label.as_deref(), Some("My tablet"));
    assert_eq!(saved[0].remote_display_name, None);
    assert_eq!(saved[0].last_authenticated_at, None);
}
