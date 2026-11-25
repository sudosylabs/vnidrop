use crate::{
    access_policy::{AccessDecision, AccessPolicy},
    util::now_ms,
    TransferAccessMode,
};

#[tokio::test]
async fn unknown_transfer_fails_closed() {
    let policy = AccessPolicy::new([]);
    assert_eq!(
        policy.decide(1, Some("node-a")).await,
        AccessDecision::Deny {
            reason: "unknown-transfer"
        }
    );
    assert_eq!(
        policy.decide(1, None).await,
        AccessDecision::Deny {
            reason: "unknown-transfer"
        }
    );
}

#[tokio::test]
async fn requires_approved_endpoint_when_locked() {
    let policy = AccessPolicy::new([]);
    policy
        .set_mode(99, TransferAccessMode::ApprovalRequired)
        .await;

    assert_eq!(
        policy.decide(99, Some("node-a")).await,
        AccessDecision::Deny {
            reason: "approval-required"
        }
    );

    policy.approve_endpoint(99, "node-a".to_string()).await;
    assert_eq!(
        policy.decide(99, Some("node-a")).await,
        AccessDecision::Allow
    );
    assert_eq!(
        policy.decide(99, None).await,
        AccessDecision::Deny {
            reason: "missing-endpoint-id"
        }
    );
}

#[tokio::test]
async fn rejects_expired_approval_sessions() {
    let policy = AccessPolicy::new([]);
    policy
        .set_mode(100, TransferAccessMode::ApprovalRequired)
        .await;
    policy
        .approve_endpoint_until(100, "node-a".to_string(), Some(now_ms() - 1))
        .await;

    assert_eq!(
        policy.decide(100, Some("node-a")).await,
        AccessDecision::Deny {
            reason: "approval-expired"
        }
    );
    assert_eq!(
        policy.decide(100, Some("node-a")).await,
        AccessDecision::Deny {
            reason: "approval-required"
        }
    );
}

#[tokio::test]
async fn public_mode_allows_without_session() {
    let policy = AccessPolicy::new([]);
    policy.set_mode(7, TransferAccessMode::Public).await;
    assert_eq!(
        policy.decide(7, Some("node-a")).await,
        AccessDecision::Allow
    );
    assert_eq!(policy.decide(7, None).await, AccessDecision::Allow);
}

#[tokio::test]
async fn approve_endpoint_grants_time_limited_session() {
    let policy = AccessPolicy::new([]);
    policy
        .set_mode(11, TransferAccessMode::ApprovalRequired)
        .await;
    policy.approve_endpoint(11, "node-b".to_string()).await;
    assert_eq!(
        policy.decide(11, Some("node-b")).await,
        AccessDecision::Allow
    );
}

#[tokio::test]
async fn block_denies_public_and_approved_requests_without_affecting_other_peers() {
    let temp = tempfile::tempdir().unwrap();
    let store = crate::persistence::open_all(temp.path())
        .await
        .unwrap()
        .blocked;
    let policy = AccessPolicy::new([]);
    policy.set_mode(1, TransferAccessMode::Public).await;
    policy
        .set_mode(2, TransferAccessMode::ApprovalRequired)
        .await;
    assert!(policy.approve_endpoint(2, "peer-a".to_owned()).await);
    assert!(policy.approve_endpoint(2, "peer-b".to_owned()).await);
    let other_peer = policy.authorize(2, Some("peer-b")).await.unwrap();

    policy.block_endpoint(&store, "peer-a").await.unwrap();
    for transfer_id in [1, 2] {
        assert!(policy.authorize(transfer_id, Some("peer-a")).await.is_err());
    }
    assert!(!policy.approve_endpoint(2, "peer-a".to_owned()).await);
    assert!(other_peer.is_active());

    let restarted = AccessPolicy::new(store.list_blocked().await.unwrap());
    restarted.set_mode(1, TransferAccessMode::Public).await;
    assert!(restarted.authorize(1, Some("peer-a")).await.is_err());

    policy.unblock_endpoint(&store, "peer-a").await.unwrap();
    assert!(policy.authorize(1, Some("peer-a")).await.is_ok());
    assert_eq!(
        policy.decide(2, Some("peer-a")).await,
        AccessDecision::Deny {
            reason: "approval-required"
        }
    );
}
