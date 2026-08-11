use crate::{blocked_devices::BlockStore, invitation::Repository, persistence};

async fn store(temp: &tempfile::TempDir) -> BlockStore {
    persistence::open_all(temp.path()).await.unwrap().blocked
}

#[tokio::test]
async fn block_list_persists_and_unblocks() {
    let temp = tempfile::tempdir().unwrap();
    let blocks = store(&temp).await;

    assert!(!blocks.is_blocked("peer-a").await.unwrap());
    blocks.block_endpoint("peer-a", 100).await.unwrap();
    assert!(blocks.is_blocked("peer-a").await.unwrap());
    assert_eq!(
        blocks.list_blocked().await.unwrap(),
        vec!["peer-a".to_string()]
    );

    blocks.unblock_endpoint("peer-a").await.unwrap();
    assert!(!blocks.is_blocked("peer-a").await.unwrap());
    assert!(blocks.list_blocked().await.unwrap().is_empty());
}

#[tokio::test]
async fn opening_app_data_drops_unreleased_prototype_tables() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("vnidrop.sqlite3");
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db)
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options).await.unwrap();
        for ddl in [
            "CREATE TABLE contacts (endpoint_id TEXT PRIMARY KEY)",
            "CREATE TABLE grants_issued (grant_id TEXT PRIMARY KEY, grant_secret TEXT NOT NULL)",
            "CREATE TABLE grants_held (grant_id TEXT PRIMARY KEY, grant_secret TEXT NOT NULL)",
            "CREATE TABLE held_offers (offer_id TEXT PRIMARY KEY, ticket TEXT NOT NULL)",
            "CREATE TABLE blocked_endpoints (endpoint_id TEXT PRIMARY KEY, created_at INTEGER NOT NULL)",
            "INSERT INTO blocked_endpoints (endpoint_id, created_at) VALUES ('keep-me', 1)",
            "INSERT INTO held_offers (offer_id, ticket) VALUES ('orphan', 'ticket')",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
    }

    let stores = persistence::open_all(temp.path()).await.unwrap();
    let pool = {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(temp.path().join("vnidrop.sqlite3"))
            .create_if_missing(false);
        sqlx::SqlitePool::connect_with(options).await.unwrap()
    };
    for table in ["contacts", "grants_issued", "grants_held", "held_offers"] {
        let row = sqlx::query(&format!(
            "SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = '{table}'"
        ))
        .fetch_one(&pool)
        .await
        .unwrap();
        let n: i64 = sqlx::Row::get(&row, "n");
        assert_eq!(n, 0, "{table} must be dropped without migration");
    }

    assert!(stores.blocked.is_blocked("keep-me").await.unwrap());
    // Invitation repository remains reachable from the bag.
    let _ = Repository::open(temp.path()).await.unwrap();
}
