// The functions here are `pub` because they are called from other test
// modules (in_memory::tests, persistent::tests). The `pub(crate)` module makes
// them accessible crate-wide; the `pub` on each function is technically
// unreachable from outside the crate, so we allow the lint.
#![allow(unreachable_pub)]

use crate::store::*;
use crate::{
    IndexDefinition, Link, LinkDirection, LinkQueryOptions, ListEventsOptions, ListObjectsOptions,
    MemoryEvent, MemoryObject, MigrationRecord, PutObjectOptions, QueueClaimOptions, QueueJob,
    QueueJobStatus, QueueNackOptions, SearchOptions, StoredSchema,
};
#[cfg(feature = "vectors")]
use crate::{VectorSearchHit, VectorSearchOptions};

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

/// Verify schema metadata and migration records survive the storage adapter.
pub fn test_contract_schema_store(engine: &mut impl ThingStore) {
    assert!(engine.get_schema_document().unwrap().is_none());
    engine
        .put_schema_document(StoredSchema {
            schema_json: "{\"version\":1}".to_string(),
            hash: "sha256:test".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(
        engine.get_schema_document().unwrap().unwrap().hash,
        "sha256:test"
    );
    engine
        .record_migration(MigrationRecord {
            id: "0001_initial.thingd".to_string(),
            hash: "sha256:test".to_string(),
            applied_at: "2026-01-01T00:00:00Z".to_string(),
        })
        .unwrap();
    assert_eq!(engine.list_migrations().unwrap().len(), 1);
}

/// Verify functional index definitions and unique-value enforcement.
pub fn test_contract_indexes(engine: &mut impl ThingStore) {
    engine
        .create_index_definition(IndexDefinition {
            collection: "users".to_string(),
            field: "email".to_string(),
            unique: true,
        })
        .unwrap();
    engine
        .put_object(MemoryObject::new(
            "users",
            "alice",
            r#"{"email":"alice@example.com"}"#,
        ))
        .unwrap();
    let duplicate = engine.put_object(MemoryObject::new(
        "users",
        "other",
        r#"{"email":"alice@example.com"}"#,
    ));
    assert!(matches!(duplicate, Err(crate::ThingdError::Conflict(_))));
    assert_eq!(
        engine.list_indexes().unwrap(),
        vec![("users".into(), "email".into())]
    );
    assert!(engine.delete_index("users", "email").unwrap());
    assert!(engine.list_indexes().unwrap().is_empty());
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

/// Run a deterministic cross-adapter scenario and return a logical digest.
///
/// Timestamps and backend-specific search scores are intentionally excluded so
/// the digest compares semantics rather than clock or indexing implementation
/// details.
pub fn run_differential_scenario(
    engine: &mut impl ThingStore,
) -> crate::ThingdResult<serde_json::Value> {
    engine.put_objects_batch(vec![
        MemoryObject::new("docs", "a", r#"{"kind":"guide","rank":1}"#),
        MemoryObject::new("docs", "b", r#"{"kind":"guide","rank":2}"#),
    ])?;
    let updated = engine.put_object_with_options(
        MemoryObject::new("docs", "a", r#"{"kind":"guide","rank":3}"#),
        PutObjectOptions {
            expected_version: Some(1),
            index: true,
        },
    )?;

    let first_event = engine.append_event(MemoryEvent::new("audit", "created", r#"{"id":"a"}"#))?;
    let second_event =
        engine.append_event(MemoryEvent::new("audit", "updated", r#"{"id":"a"}"#))?;

    engine.push_job(QueueJob::new("jobs", "job-1", r#"{"task":"index"}"#, 3))?;
    let claimed = engine
        .claim_job_with_options("jobs", QueueClaimOptions::new(60_000))?
        .ok_or_else(|| crate::ThingdError::Storage("differential queue claim missing".into()))?;
    let completed = engine
        .ack_job("jobs", &claimed.id)?
        .ok_or_else(|| crate::ThingdError::Storage("differential queue ack missing".into()))?;

    let link = engine.create_link(
        Link::new("docs/a", "supports", "docs/b")
            .with_weight(0.5)
            .with_metadata(r#"{"source":"test"}"#),
    )?;
    let neighbors = engine.get_neighbors(
        "docs/a",
        LinkDirection::Outgoing,
        LinkQueryOptions::default(),
    )?;

    engine.put_schema_document(StoredSchema {
        schema_json: r#"{"version":1}"#.to_string(),
        hash: "schema-hash".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    })?;
    engine.record_migration(MigrationRecord {
        id: "0001_test".to_string(),
        hash: "schema-hash".to_string(),
        applied_at: "2026-01-01T00:00:00Z".to_string(),
    })?;
    engine.create_index_definition(IndexDefinition {
        collection: "docs".to_string(),
        field: "kind".to_string(),
        unique: false,
    })?;

    #[cfg(feature = "vectors")]
    {
        engine.add_vector("docs", "a", &[1.0, 0.0])?;
    }

    let objects = engine
        .list_objects(None, &ListObjectsOptions::default())?
        .into_iter()
        .map(|object| {
            serde_json::json!({
                "collection": object.key.collection,
                "id": object.key.id,
                "body": object.body,
                "version": object.version,
            })
        })
        .collect::<Vec<_>>();
    let events = engine
        .list_events(Some("audit"), ListEventsOptions::default())?
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "stream": event.stream,
                "sequence": event.sequence,
                "eventType": event.event_type,
                "body": event.body,
            })
        })
        .collect::<Vec<_>>();
    let jobs = engine
        .list_jobs("jobs")?
        .into_iter()
        .map(|job| {
            serde_json::json!({
                "queue": job.queue,
                "id": job.id,
                "body": job.body,
                "attempts": job.attempts,
                "maxAttempts": job.max_attempts,
                "status": format!("{:?}", job.status),
                "lastError": job.last_error,
            })
        })
        .collect::<Vec<_>>();
    let search_ids = engine
        .search("guide", SearchOptions::default())?
        .into_iter()
        .map(|hit| format!("{}:{}/{}", hit.kind, hit.collection, hit.id))
        .collect::<Vec<_>>();
    let index_definitions = engine.list_index_definitions()?;
    let schema_hash = engine.get_schema_document()?.map(|schema| schema.hash);
    let migrations = engine
        .list_migrations()?
        .into_iter()
        .map(|migration| migration.id)
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "objects": objects,
        "updatedVersion": updated.version,
        "events": events,
        "eventSequences": [first_event.sequence, second_event.sequence],
        "jobs": jobs,
        "completedStatus": format!("{:?}", completed.status),
        "link": {
            "id": link.id,
            "from": link.from_ref,
            "type": link.link_type,
            "to": link.to_ref,
            "weight": link.weight,
            "metadata": link.metadata_json,
        },
        "neighborIds": neighbors.into_iter().map(|link| link.id).collect::<Vec<_>>(),
        "schemaHash": schema_hash,
        "migrations": migrations,
        "indexes": index_definitions,
        "searchIds": search_ids,
        "counts": {
            "objects": engine.count_objects()?,
            "events": engine.count_events()?,
            "links": engine.count_links()?,
            "queues": engine.list_queues()?.len(),
            "activeJobs": engine.count_active_jobs()?,
            "deadJobs": engine.count_dead_jobs()?,
        },
    }))
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
