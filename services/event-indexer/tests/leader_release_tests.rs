//! Leader lease release on graceful shutdown (#2003).
//!
//! `LeaderElection::release` is the shutdown path: it deletes the single
//! `leader_state` row so a follower can take over immediately instead of
//! waiting for the lease TTL to expire.
//!
//! These tests need a live PostgreSQL instance. They are skipped unless
//! `DATABASE_URL` is set, matching the other Postgres tests in this crate.

use event_indexer::db::Database;
use event_indexer::leader::LeaderElection;

const LEASE_TTL_SECS: u64 = 30;
const HEARTBEAT_SECS: u64 = 5;

/// Connect, create the schema, and clear any lease left by a previous run.
async fn fresh_database() -> Option<Database> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let db = Database::from_dsns(&url, &url, 2, 2).expect("DATABASE_URL must parse");
    db.init_schema().await.expect("schema init");
    let conn = db.write_pool().get().await.expect("pool connection");
    conn.execute("DELETE FROM leader_state", &[])
        .await
        .expect("clear leader_state");
    Some(db)
}

async fn lease_row_count(db: &Database) -> i64 {
    let conn = db.write_pool().get().await.expect("pool connection");
    conn.query_one(
        "SELECT COUNT(*) FROM leader_state WHERE instance_id = '__leader__'",
        &[],
    )
    .await
    .expect("count query")
    .get(0)
}

fn election(db: &Database, instance_id: &str) -> LeaderElection {
    LeaderElection::new(
        db.write_pool().clone(),
        instance_id.to_string(),
        LEASE_TTL_SECS,
        HEARTBEAT_SECS,
    )
}

#[tokio::test]
async fn release_on_shutdown_deletes_the_lease_row() {
    let Some(db) = fresh_database().await else {
        println!("Skipping release_on_shutdown_deletes_the_lease_row: DATABASE_URL not set");
        return;
    };

    let mut leader = election(&db, "instance-a");
    assert!(leader.try_acquire().await, "first instance must acquire the lease");
    assert_eq!(lease_row_count(&db).await, 1);

    leader.release().await;

    assert!(!leader.is_leader(), "release must clear local leadership");
    assert_eq!(lease_row_count(&db).await, 0, "release must delete the lease row");
}

#[tokio::test]
async fn new_leader_acquires_immediately_after_release() {
    let Some(db) = fresh_database().await else {
        println!("Skipping new_leader_acquires_immediately_after_release: DATABASE_URL not set");
        return;
    };

    let mut old_leader = election(&db, "instance-a");
    assert!(old_leader.try_acquire().await);
    old_leader.release().await;

    // The lease TTL is 30s, so a follower succeeding here proves it did not wait for expiry.
    let mut new_leader = election(&db, "instance-b");
    assert!(
        new_leader.try_acquire().await,
        "a follower must acquire the lease right after the leader releases it"
    );
    assert!(new_leader.is_leader());

    new_leader.release().await;
}
