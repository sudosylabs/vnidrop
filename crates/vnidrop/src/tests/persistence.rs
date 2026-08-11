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
