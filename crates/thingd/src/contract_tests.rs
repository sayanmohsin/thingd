// The functions here are `pub` because they are called from other test
// modules (in_memory::tests, persistent::tests). The `pub(crate)` module makes
// them accessible crate-wide; the `pub` on each function is technically
// unreachable from outside the crate, so we allow the lint.
#![allow(unreachable_pub)]

use crate::store::*;
use crate::{
    MemoryEvent, MemoryObject, QueueClaimOptions, QueueJob, QueueJobStatus, QueueNackOptions,
    SearchOptions, VectorSearchHit, VectorSearchOptions,
};

/// Verify object CRUD lifecycle: create, read, update, delete.
pub fn test_contract_object_lifecycle(engine: &mut impl ThingStore) {
    let obj = engine
        .put_object(MemoryObject::new("col", "a", r#"{"v":1}"#))
        .unwrap();
    assert_eq!(obj.version, 1);

    let stored = engine.get_object("col", "a").unwrap().unwrap();
    assert_eq!(stored.body, r#"{"v":1}"#);

    let updated = engine
        .put_object(MemoryObject::new("col", "a", r#"{"v":2}"#))
        .unwrap();
    assert_eq!(updated.version, 2);

    assert!(engine.delete_object("col", "a").unwrap());
    assert!(engine.get_object("col", "a").unwrap().is_none());
}

/// Verify vector lifecycle: add, update (preserves), remove on update to None.
pub fn test_contract_vector_lifecycle(engine: &mut impl ThingStore) {
    engine
        .put_object(MemoryObject::new("col", "a", r#"{"v":1}"#).with_vector(vec![1.0, 0.0]))
        .unwrap();

    #[cfg(feature = "vectors")]
    {
        let hits: Vec<VectorSearchHit> = engine
            .vector_search("col", &[1.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert_eq!(hits.len(), 1, "vector search should find object");
    }

    // Update without vector — old vector must be removed
    engine
        .put_object(MemoryObject::new("col", "a", r#"{"v":2}"#))
        .unwrap();

    #[cfg(feature = "vectors")]
    {
        let hits: Vec<VectorSearchHit> = engine
            .vector_search("col", &[1.0, 0.0], VectorSearchOptions::default())
            .unwrap();
        assert_eq!(
            hits.len(),
            0,
            "vector must be removed when object updated without vector"
        );
    }
}

/// Verify event append with idempotency and sequence ordering.
pub fn test_contract_event_idempotency(engine: &mut impl ThingStore) {
    let mut e1 = MemoryEvent::new("s", "type-a", r#"{"n":1}"#);
    e1.idempotency_key = "k1".to_string();
    let e1 = engine.append_event(e1).unwrap();
    assert_eq!(e1.sequence, 1);

    // Duplicate idempotency key — must return same event
    let mut e2 = MemoryEvent::new("s", "type-a", r#"{"n":1}"#);
    e2.idempotency_key = "k1".to_string();
    let e2 = engine.append_event(e2).unwrap();
    assert_eq!(e2.sequence, 1);

    // New idempotency key — must get next sequence
    let mut e3 = MemoryEvent::new("s", "type-b", r#"{"n":2}"#);
    e3.idempotency_key = "k2".to_string();
    let e3 = engine.append_event(e3).unwrap();
    assert_eq!(e3.sequence, 2);

    // Events without idempotency key are always appended
    let e4 = engine
        .append_event(MemoryEvent::new("s", "type-c", r#"{"n":3}"#))
        .unwrap();
    assert_eq!(e4.sequence, 3);

    let e5 = engine
        .append_event(MemoryEvent::new("s", "type-d", r#"{"n":4}"#))
        .unwrap();
    assert_eq!(e5.sequence, 4);
}

/// Verify queue push, claim, ack, nack lifecycle.
pub fn test_contract_queue_lifecycle(engine: &mut impl ThingStore) {
    let job = engine
        .push_job(QueueJob::new("q", "j1", r#"{"task":"test"}"#, 3))
        .unwrap();
    assert_eq!(job.status, QueueJobStatus::Ready);

    // Claim returns the available job
    let claimed = engine
        .claim_job_with_options("q", QueueClaimOptions::default())
        .unwrap()
        .expect("should claim job");
    assert_eq!(claimed.id, "j1");
    assert_eq!(claimed.status, QueueJobStatus::Leased);

    // Ack completes the job
    let acked = engine.ack_job("q", "j1").unwrap().unwrap();
    assert_eq!(acked.status, QueueJobStatus::Completed);

    // No more jobs to claim
    assert!(
        engine
            .claim_job_with_options("q", QueueClaimOptions::default())
            .unwrap()
            .is_none()
    );
}

/// Verify delayed jobs are not claimable before their availability time.
pub fn test_contract_delayed_job(engine: &mut impl ThingStore) {
    let far_future = 99_999_999_999_999i64;
    let mut job = QueueJob::new("q", "d1", r"{}", 3);
    job.available_at_ms = far_future;
    engine.push_job(job).unwrap();

    // Immediate claim — delayed job should not appear
    assert!(
        engine
            .claim_job_with_options("q", QueueClaimOptions::default())
            .unwrap()
            .is_none()
    );

    // Push an immediately available job
    let mut now_job = QueueJob::new("q", "now1", r"{}", 3);
    now_job.available_at_ms = 0;
    engine.push_job(now_job).unwrap();

    // Claim returns the available job, not the delayed one
    let claimed = engine
        .claim_job_with_options("q", QueueClaimOptions::default())
        .unwrap()
        .expect("should claim available job");
    assert_eq!(claimed.id, "now1");
}

/// Verify lease expiration causes jobs to become reclaimable.
pub fn test_contract_lease_expiration(engine: &mut impl ThingStore) {
    let job = engine
        .push_job(QueueJob::new("q", "l1", r#"{"x":1}"#, 3))
        .unwrap();
    assert_eq!(job.status, QueueJobStatus::Ready);

    // Claim with a very short lease
    let _claimed = engine
        .claim_job_with_options("q", QueueClaimOptions { lease_ms: 1 })
        .unwrap()
        .expect("should claim");

    // Wait for lease to expire
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Should be reclaimable now
    let reclaimed = engine
        .claim_job_with_options("q", QueueClaimOptions::default())
        .unwrap()
        .expect("lease should have expired");
    assert_eq!(reclaimed.id, "l1");
}

/// Verify nack with retry delay and dead-letter after max attempts.
pub fn test_contract_nack_dead_letter(engine: &mut impl ThingStore) {
    let job = QueueJob::new("q", "n1", r#"{"x":1}"#, 2);
    engine.push_job(job).unwrap();

    // Claim, nack with delay
    let _claimed = engine
        .claim_job_with_options("q", QueueClaimOptions::default())
        .unwrap()
        .expect("should claim");
    let nacked = engine
        .nack_job_with_options(
            "q",
            "n1",
            QueueNackOptions {
                delay_ms: 0,
                error: "retry".to_string(),
            },
        )
        .unwrap()
        .expect("should nack");
    assert_eq!(nacked.status, QueueJobStatus::Ready);
    assert_eq!(nacked.attempts, 1);

    // Second claim and nack — reaches max_attempts
    let _claimed2 = engine
        .claim_job_with_options("q", QueueClaimOptions::default())
        .unwrap()
        .expect("should claim again");
    let dead = engine
        .nack_job_with_options(
            "q",
            "n1",
            QueueNackOptions {
                delay_ms: 0,
                error: "final".to_string(),
            },
        )
        .unwrap()
        .expect("should nack");
    assert_eq!(dead.status, QueueJobStatus::Dead);
    assert!(!engine.list_dead_jobs("q").unwrap().is_empty());
}

/// Verify search returns correct results (basic smoke test).
pub fn test_contract_search(engine: &mut impl ThingStore) {
    engine
        .put_object(MemoryObject::new("docs", "d1", r#"{"text":"hello world"}"#))
        .unwrap();
    engine
        .put_object(MemoryObject::new(
            "docs",
            "d2",
            r#"{"text":"goodbye world"}"#,
        ))
        .unwrap();

    let hits = engine.search("hello", SearchOptions::default()).unwrap();
    assert!(
        hits.iter().map(|h| h.id.as_str()).any(|id| id == "d1"),
        "search should find 'hello' in d1"
    );
}
