//! Tests for leader election advisory lock release on panic/drop.
//!
//! Tests verify:
//! - Deterministic advisory lock key constant exists
//! - `release_advisory_lock` releases session lock and cleans leader row
//! - Dropping a leader instance (simulating panic or abnormal shutdown) triggers lock release

use deadpool_postgres::{Config as PoolConfig, Runtime};
use event_indexer::{db::Database, leader::{self, LeaderElection, LOCK_KEY}};
use std::sync::Arc;
use tokio_postgres::NoTls;

#[test]
fn advisory_lock_key_is_deterministic_constant() {
    assert_eq!(LOCK_KEY, 0x65_76_74_69_64_78);
}

#[tokio::test]
async fn leader_advisory_lock_released_on_drop() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping leader_advisory_lock_released_on_drop: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let mut cfg = PoolConfig::new();
    cfg.url = Some(db_url.clone());
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("create pool");

    // Clean up any existing leader state or advisory locks first
    let _ = leader::release_advisory_lock(&pool).await;

    // Acquire leadership with leader 1
    let mut leader1 = LeaderElection::new(pool.clone(), "inst-test-1".to_string(), 30, 5);
    let acquired = leader1.try_acquire().await;
    assert!(acquired, "leader 1 must acquire leadership and advisory lock");
    assert!(leader1.is_leader());

    // Second leader should fail to acquire because advisory lock / lease is held
    let mut leader2 = LeaderElection::new(pool.clone(), "inst-test-2".to_string(), 30, 5);
    let acquired2 = leader2.try_acquire().await;
    assert!(!acquired2, "leader 2 must not acquire while leader 1 is active");

    // Drop leader 1 (simulating panic or crash)
    drop(leader1);

    // Yield to let the asynchronous drop cleanup task run
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // Now leader 2 should be able to acquire because the drop handler released the lock
    let acquired2_after = leader2.try_acquire().await;
    assert!(
        acquired2_after,
        "leader 2 must acquire leadership after leader 1 is dropped"
    );

    // Clean up
    leader2.release().await;
}

#[tokio::test]
async fn leader_release_advisory_lock_helper_works() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping leader_release_advisory_lock_helper_works: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let mut cfg = PoolConfig::new();
    cfg.url = Some(db_url.clone());
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls).expect("create pool");

    // Call helper
    let res = leader::release_advisory_lock(&pool).await;
    assert!(res.is_ok(), "release_advisory_lock helper must succeed");
}
