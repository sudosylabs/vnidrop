//! Persistence open returns domain stores without exporting a raw pool to callers.

use crate::persistence;

#[tokio::test]
async fn open_all_returns_invitation_targeted_and_blocked_stores() {
    let temp = tempfile::tempdir().unwrap();
    let stores = persistence::open_all(temp.path()).await.unwrap();

    assert!(stores.blocked.list_blocked().await.unwrap().is_empty());
    assert!(stores.targeted.list().await.unwrap().is_empty());
    assert!(stores.invitation.list_transfers().await.unwrap().is_empty());
}
