//! Provider-neutral replication primitives shared by HTTP and native callers.
//!
//! The service in this module deliberately owns protocol semantics, not
//! transport or Cloud policy. A caller supplies a [`ThingStore`] and may use
//! the same source-feed, snapshot, and replica-apply behavior from an HTTP
//! handler, an embedded Rust application, or a future native binding.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    ListEventsOptions, ListObjectsOptions, MemoryEvent, MemoryObject, PutObjectOptions, ThingStore,
    ThingdError, ThingdResult,
};

/// The protected event stream containing source changes.
pub const REPLICATION_STREAM: &str = "__thingd:system:replication";
/// The protected collection containing target checkpoints.
pub const REPLICATION_STATE_COLLECTION: &str = "__thingd:sync_state";
/// The protected collection containing source provenance.
pub const REPLICATION_PROVENANCE_COLLECTION: &str = "__thingd:sync_provenance";
/// The protected collection containing source deletion tombstones.
pub const REPLICATION_TOMBSTONE_COLLECTION: &str = "__thingd:sync_tombstones";
/// The protected collection containing quarantined conflicts.
pub const REPLICATION_QUARANTINE_COLLECTION: &str = "__thingd:sync_conflicts";

/// Whether a store accepts normal writes or replication applies.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationRole {
    /// The store emits changes and rejects replica-only writes.
    #[default]
    Source,
    /// The store accepts changes from one authoritative source.
    Replica,
}

/// Engine-level replication configuration.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationConfig {
    /// Stable identity included in every source change.
    pub source_id: String,
    /// Current source or replica role.
    pub role: ReplicationRole,
    /// Optional allowlist. Empty means all non-system collections.
    #[serde(default)]
    pub collections: Vec<String>,
}

impl ReplicationConfig {
    /// Create source configuration with a stable source identity.
    pub fn source(source_id: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
            role: ReplicationRole::Source,
            collections: Vec::new(),
        }
    }

    /// Create replica configuration for a target store.
    pub fn replica(source_id: impl Into<String>) -> Self {
        Self {
            source_id: source_id.into(),
            role: ReplicationRole::Replica,
            collections: Vec::new(),
        }
    }

    fn allowed(&self, collection: &str) -> bool {
        !collection.starts_with("__thingd")
            && (self.collections.is_empty()
                || self.collections.iter().any(|allowed| allowed == collection))
    }
}

/// A single durable source change in the public replication envelope.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationChange {
    /// Source identity.
    pub source_id: String,
    /// Source replication-stream sequence.
    pub cursor: u64,
    /// Stable retry key derived from source and cursor.
    pub idempotency_key: String,
    /// Provider-neutral operation payload.
    pub change: Value,
}

/// A page from a source replication feed.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationPage {
    /// Source identity.
    pub source_id: String,
    /// Cursor requested by the caller.
    pub after: u64,
    /// Cursor of the last returned change.
    pub next: u64,
    /// Returned changes in ascending cursor order.
    pub changes: Vec<ReplicationChange>,
}

/// A bootstrap snapshot for a stale or new replica.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationSnapshot {
    /// Source identity.
    pub source_id: String,
    /// Source replication cursor represented by this snapshot.
    pub cursor: u64,
    /// Replicable objects.
    pub objects: Vec<MemoryObject>,
    /// Application events, excluding protected system streams.
    pub events: Vec<MemoryEvent>,
}

/// Result of applying a change batch.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationApplyResult {
    /// Number of changes applied.
    pub applied: u64,
    /// Number of duplicate or filtered changes skipped.
    pub skipped: u64,
    /// Number of quarantined conflicts in this batch.
    pub conflicts: u64,
    /// Last durable target cursor.
    pub last_applied_cursor: u64,
}

/// Current source or replica replication state.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplicationStatus {
    /// Configured source identity.
    pub source_id: String,
    /// Configured role.
    pub role: ReplicationRole,
    /// Latest source cursor.
    pub latest_cursor: u64,
    /// Number of source changes retained.
    pub change_count: u64,
    /// Last cursor applied for the configured source.
    pub last_applied_cursor: u64,
    /// Number of quarantined conflicts.
    pub quarantined_conflicts: u64,
}

/// Backend-neutral replication operations over a Thingd store.
pub struct ReplicationService<'a> {
    engine: &'a mut dyn ThingStore,
    config: ReplicationConfig,
}

impl<'a> ReplicationService<'a> {
    /// Create a service over an existing in-memory or persistent engine.
    pub fn new(engine: &'a mut dyn ThingStore, config: ReplicationConfig) -> Self {
        Self { engine, config }
    }

    /// Return the service configuration.
    pub const fn config(&self) -> &ReplicationConfig {
        &self.config
    }

    /// Record an object upsert in the durable source feed.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not configured as a source or the
    /// durable replication event cannot be appended.
    pub fn record_object_upsert(&mut self, object: &MemoryObject) -> ThingdResult<()> {
        if !self.config.allowed(&object.key.collection) {
            return Ok(());
        }
        self.record_change(&json!({
            "operation": "object.upsert",
            "collection": object.key.collection,
            "id": object.key.id,
            "payload": {
                "id": object.key.id,
                "collection": object.key.collection,
                "body": serde_json::from_str::<Value>(&object.body).unwrap_or(Value::Null),
                "version": object.version,
                "createdAt": object.created_at,
                "updatedAt": object.updated_at,
            },
        }))
    }

    /// Record an object deletion in the durable source feed.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not configured as a source or the
    /// durable replication event cannot be appended.
    pub fn record_object_delete(&mut self, collection: &str, id: &str) -> ThingdResult<()> {
        if !self.config.allowed(collection) {
            return Ok(());
        }
        self.record_change(&json!({
            "operation": "object.delete",
            "collection": collection,
            "id": id,
            "payload": Value::Null,
        }))
    }

    /// Record an application event in the durable source feed.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not configured as a source or the
    /// durable replication event cannot be appended.
    pub fn record_event_append(&mut self, event: &MemoryEvent) -> ThingdResult<()> {
        if event.stream.starts_with("__thingd") {
            return Ok(());
        }
        self.record_change(&json!({
            "operation": "event.append",
            "collection": Value::Null,
            "id": Value::Null,
            "payload": {
                "stream": event.stream,
                "type": event.event_type,
                "body": serde_json::from_str::<Value>(&event.body).unwrap_or(Value::Null),
                "idempotencyKey": event.idempotency_key,
            },
        }))
    }

    /// Record a provider-neutral change payload in the source feed.
    ///
    /// This is the escape hatch for transports that already construct the
    /// public operation envelope. Prefer the typed `record_*` methods for
    /// native callers.
    ///
    /// # Errors
    ///
    /// Returns an error if the service is not configured as a source or the
    /// durable replication event cannot be appended.
    pub fn record_change(&mut self, change: &Value) -> ThingdResult<()> {
        if self.config.role != ReplicationRole::Source {
            return Err(ThingdError::InvalidInput(
                "replication changes can only be recorded by a source".to_string(),
            ));
        }
        self.engine.append_event(MemoryEvent::new(
            REPLICATION_STREAM,
            change
                .get("operation")
                .and_then(Value::as_str)
                .unwrap_or("change"),
            change.to_string(),
        ))?;
        Ok(())
    }

    /// Read a source page after a cursor.
    ///
    /// # Errors
    ///
    /// Returns an error when the cursor has fallen outside the retained
    /// replication event window or the event store cannot be read.
    pub fn events(&mut self, after: u64, limit: u64) -> ThingdResult<ReplicationPage> {
        let limit = limit.clamp(1, 1_000);
        if after > 0 {
            let first = self.engine.list_events(
                Some(REPLICATION_STREAM),
                ListEventsOptions {
                    limit: Some(1),
                    ..Default::default()
                },
            )?;
            if let Some(first) = first.first()
                && first.sequence > after.saturating_add(1)
            {
                return Err(ThingdError::Conflict(
                    "replication cursor is no longer available; bootstrap from a snapshot"
                        .to_string(),
                ));
            }
        }
        let events = self.engine.list_events(
            Some(REPLICATION_STREAM),
            ListEventsOptions {
                from_sequence: Some(after),
                limit: Some(limit),
                ..Default::default()
            },
        )?;
        let changes = events
            .iter()
            .filter_map(|event| {
                serde_json::from_str::<Value>(&event.body)
                    .ok()
                    .map(|change| ReplicationChange {
                        source_id: self.config.source_id.clone(),
                        cursor: event.sequence,
                        idempotency_key: format!("{}:{}", self.config.source_id, event.sequence),
                        change,
                    })
            })
            .collect::<Vec<_>>();
        Ok(ReplicationPage {
            source_id: self.config.source_id.clone(),
            after,
            next: events.last().map_or(after, |event| event.sequence),
            changes,
        })
    }

    /// Create a source snapshot containing only replicable data.
    ///
    /// # Errors
    ///
    /// Returns an error when the object or event stores cannot be read.
    pub fn snapshot(&mut self) -> ThingdResult<ReplicationSnapshot> {
        let objects = self
            .engine
            .list_objects(
                None,
                &ListObjectsOptions {
                    limit: Some(100_000),
                    ..Default::default()
                },
            )?
            .into_iter()
            .filter(|object| self.config.allowed(&object.key.collection))
            .collect();
        let events = self
            .engine
            .list_events(
                None,
                ListEventsOptions {
                    limit: Some(100_000),
                    ..Default::default()
                },
            )?
            .into_iter()
            .filter(|event| !event.stream.starts_with("__thingd"))
            .collect();
        let cursor = self
            .engine
            .list_events(
                Some(REPLICATION_STREAM),
                ListEventsOptions {
                    limit: Some(100_000),
                    ..Default::default()
                },
            )?
            .last()
            .map_or(0, |event| event.sequence);
        Ok(ReplicationSnapshot {
            source_id: self.config.source_id.clone(),
            cursor,
            objects,
            events,
        })
    }

    /// Apply a source batch to a replica store.
    ///
    /// # Errors
    ///
    /// Returns an error for source-role stores, malformed changes, conflicts,
    /// or durable storage failures.
    pub fn apply(&mut self, changes: &[ReplicationChange]) -> ThingdResult<ReplicationApplyResult> {
        if self.config.role != ReplicationRole::Replica {
            return Err(ThingdError::InvalidInput(
                "replication apply requires a replica role".to_string(),
            ));
        }
        let mut result = ReplicationApplyResult::default();
        let mut source_id: Option<&str> = None;
        let mut checkpoint = 0;
        for item in changes {
            if let Some(existing) = source_id
                && existing != item.source_id
            {
                return Err(ThingdError::InvalidInput(
                    "a replication batch cannot contain multiple source IDs".to_string(),
                ));
            }
            source_id = Some(item.source_id.as_str());
            if checkpoint == 0 {
                checkpoint = self.read_applied_cursor(&item.source_id)?;
            }
            if item.cursor > 0 && item.cursor <= checkpoint {
                result.skipped += 1;
                continue;
            }
            let change = &item.change;
            let operation = change
                .get("operation")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ThingdError::InvalidInput("replication change is missing operation".to_string())
                })?;
            let collection = change.get("collection").and_then(Value::as_str);
            if let Some(collection) = collection
                && !self.config.allowed(collection)
            {
                result.skipped += 1;
                result.last_applied_cursor = result.last_applied_cursor.max(item.cursor);
                continue;
            }
            match operation {
                "object.upsert" => self.apply_object(item, collection)?,
                "object.delete" => self.apply_delete(item, collection)?,
                "event.append" => self.apply_event(item)?,
                other => {
                    return Err(ThingdError::InvalidInput(format!(
                        "unknown replication operation: {other}"
                    )));
                },
            }
            result.applied += 1;
            result.last_applied_cursor = result.last_applied_cursor.max(item.cursor);
        }
        if let Some(source_id) = source_id
            && result.last_applied_cursor > checkpoint
        {
            self.write_applied_cursor(source_id, result.last_applied_cursor)?;
        }
        Ok(result)
    }

    fn apply_object(
        &mut self,
        item: &ReplicationChange,
        collection: Option<&str>,
    ) -> ThingdResult<()> {
        let collection = collection
            .ok_or_else(|| ThingdError::InvalidInput("missing collection".to_string()))?;
        let id = item
            .change
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ThingdError::InvalidInput("missing object id".to_string()))?;
        let payload = item.change.get("payload").cloned().unwrap_or(Value::Null);
        let existing = self.engine.get_object(collection, id)?;
        let metadata_id = replication_metadata_id(&item.source_id, collection, id);
        if existing.is_some()
            && self
                .engine
                .get_object(REPLICATION_PROVENANCE_COLLECTION, &metadata_id)?
                .is_none()
        {
            let conflict = json!({
                "operation": "object.upsert",
                "collection": collection,
                "id": id,
                "reason": "target_object_has_no_replication_provenance",
            });
            self.write_conflict(&item.source_id, item.cursor, &conflict)?;
            return Err(ThingdError::Conflict(format!(
                "replication conflict for {collection}/{id}"
            )));
        }
        let mut object = MemoryObject::new(
            collection,
            id,
            payload
                .get("body")
                .cloned()
                .unwrap_or(Value::Null)
                .to_string(),
        );
        object.version = payload.get("version").and_then(Value::as_u64).unwrap_or(0);
        object.created_at = payload
            .get("createdAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        object.updated_at = payload
            .get("updatedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        self.engine.put_object_with_source_metadata(
            object,
            PutObjectOptions {
                expected_version: existing.map(|value| value.version),
                ..Default::default()
            },
        )?;
        self.write_provenance(&item.source_id, item.cursor, collection, id, &payload)
    }

    fn apply_delete(
        &mut self,
        item: &ReplicationChange,
        collection: Option<&str>,
    ) -> ThingdResult<()> {
        let collection = collection
            .ok_or_else(|| ThingdError::InvalidInput("missing collection".to_string()))?;
        let id = item
            .change
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| ThingdError::InvalidInput("missing object id".to_string()))?;
        self.engine.delete_object(collection, id)?;
        self.engine.put_object(MemoryObject::new(
            REPLICATION_TOMBSTONE_COLLECTION,
            replication_metadata_id(&item.source_id, collection, id),
            json!({"sourceId": item.source_id, "cursor": item.cursor, "collection": collection, "id": id, "deleted": true}).to_string(),
        ))?;
        Ok(())
    }

    fn apply_event(&mut self, item: &ReplicationChange) -> ThingdResult<()> {
        let payload = item
            .change
            .get("payload")
            .ok_or_else(|| ThingdError::InvalidInput("missing event payload".to_string()))?;
        let stream = payload
            .get("stream")
            .and_then(Value::as_str)
            .ok_or_else(|| ThingdError::InvalidInput("missing event stream".to_string()))?;
        let event_type = payload
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| ThingdError::InvalidInput("missing event type".to_string()))?;
        let mut event = MemoryEvent::new(
            stream,
            event_type,
            payload
                .get("body")
                .cloned()
                .unwrap_or(Value::Null)
                .to_string(),
        );
        event.idempotency_key.clone_from(&item.idempotency_key);
        self.engine.append_event(event)?;
        Ok(())
    }

    /// Apply a bootstrap snapshot to a replica.
    ///
    /// # Errors
    ///
    /// Returns an error for source-role stores, conflicts, or durable storage
    /// failures.
    pub fn apply_snapshot(
        &mut self,
        snapshot: &ReplicationSnapshot,
        replace: bool,
    ) -> ThingdResult<ReplicationApplyResult> {
        if self.config.role != ReplicationRole::Replica {
            return Err(ThingdError::InvalidInput(
                "snapshot apply requires a replica role".to_string(),
            ));
        }
        if replace {
            let existing = self.engine.list_objects(
                None,
                &ListObjectsOptions {
                    limit: Some(100_000),
                    ..Default::default()
                },
            )?;
            for object in existing {
                if self.config.allowed(&object.key.collection) {
                    self.engine
                        .delete_object(&object.key.collection, &object.key.id)?;
                }
            }
        }
        let mut result = ReplicationApplyResult::default();
        for object in &snapshot.objects {
            if self.config.allowed(&object.key.collection) {
                self.engine
                    .put_object_with_source_metadata(object.clone(), PutObjectOptions::default())?;
                result.applied += 1;
            }
        }
        for event in &snapshot.events {
            if !event.stream.starts_with("__thingd") {
                self.engine.append_event(event.clone())?;
            }
        }
        self.write_applied_cursor(&snapshot.source_id, snapshot.cursor)?;
        result.last_applied_cursor = snapshot.cursor;
        Ok(result)
    }

    /// Return status for the configured source/replica.
    ///
    /// # Errors
    ///
    /// Returns an error when replication state cannot be read.
    pub fn status(&mut self) -> ThingdResult<ReplicationStatus> {
        let source_id = self.config.source_id.clone();
        let changes = self
            .engine
            .list_events(Some(REPLICATION_STREAM), ListEventsOptions::default())?;
        let conflicts = self.engine.list_objects(
            Some(&[REPLICATION_QUARANTINE_COLLECTION.to_string()]),
            &ListObjectsOptions {
                limit: Some(10_000),
                ..Default::default()
            },
        )?;
        Ok(ReplicationStatus {
            source_id: source_id.clone(),
            role: self.config.role,
            latest_cursor: changes.last().map_or(0, |event| event.sequence),
            change_count: changes.len() as u64,
            last_applied_cursor: self.read_applied_cursor(&source_id)?,
            quarantined_conflicts: conflicts.len() as u64,
        })
    }

    /// List durable quarantined conflicts.
    ///
    /// # Errors
    ///
    /// Returns an error when the conflict collection cannot be read.
    pub fn conflicts(&mut self) -> ThingdResult<Vec<MemoryObject>> {
        self.engine.list_objects(
            Some(&[REPLICATION_QUARANTINE_COLLECTION.to_string()]),
            &ListObjectsOptions {
                limit: Some(10_000),
                ..Default::default()
            },
        )
    }

    fn read_applied_cursor(&self, source_id: &str) -> ThingdResult<u64> {
        let id = format!("source:{source_id}");
        Ok(self
            .engine
            .get_object(REPLICATION_STATE_COLLECTION, &id)?
            .and_then(|object| serde_json::from_str::<Value>(&object.body).ok())
            .and_then(|body| body.get("lastAppliedCursor").and_then(Value::as_u64))
            .unwrap_or(0))
    }

    fn write_applied_cursor(&mut self, source_id: &str, cursor: u64) -> ThingdResult<()> {
        self.engine
            .put_object(MemoryObject::new(
                REPLICATION_STATE_COLLECTION,
                format!("source:{source_id}"),
                json!({"sourceId": source_id, "lastAppliedCursor": cursor}).to_string(),
            ))
            .map(|_| ())
    }

    fn write_provenance(
        &mut self,
        source_id: &str,
        cursor: u64,
        collection: &str,
        id: &str,
        payload: &Value,
    ) -> ThingdResult<()> {
        self.engine.put_object(MemoryObject::new(REPLICATION_PROVENANCE_COLLECTION, replication_metadata_id(source_id, collection, id), json!({"sourceId": source_id, "cursor": cursor, "collection": collection, "id": id, "sourceVersion": payload.get("version").and_then(Value::as_u64).unwrap_or(0), "createdAt": payload.get("createdAt").and_then(Value::as_str).unwrap_or_default(), "updatedAt": payload.get("updatedAt").and_then(Value::as_str).unwrap_or_default(), "deleted": false}).to_string())).map(|_| ())
    }

    fn write_conflict(
        &mut self,
        source_id: &str,
        cursor: u64,
        conflict: &Value,
    ) -> ThingdResult<()> {
        self.engine.put_object(MemoryObject::new(REPLICATION_QUARANTINE_COLLECTION, format!("{source_id}:{cursor}"), json!({"sourceId": source_id, "cursor": cursor, "status": "quarantined", "conflict": conflict}).to_string())).map(|_| ())
    }
}

fn replication_metadata_id(source_id: &str, collection: &str, id: &str) -> String {
    format!("{source_id}:{collection}:{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventLog, MemoryEngine, ObjectStore};

    #[cfg(feature = "persistent")]
    use crate::PersistentEngine;

    #[test]
    fn native_source_records_and_reads_object_and_event_changes() {
        let mut engine = MemoryEngine::new();
        let config = ReplicationConfig::source("native-a");
        let object = engine
            .put_object(MemoryObject::new("notes", "1", r#"{"ok":true}"#))
            .unwrap();
        let event = engine
            .append_event(MemoryEvent::new("notes", "created", r#"{"id":"1"}"#))
            .unwrap();
        let mut replication = ReplicationService::new(&mut engine, config);
        replication.record_object_upsert(&object).unwrap();
        replication.record_event_append(&event).unwrap();
        let page = replication.events(0, 100).unwrap();
        assert_eq!(page.source_id, "native-a");
        assert_eq!(page.changes.len(), 2);
        assert_eq!(page.changes[0].change["operation"], "object.upsert");
        assert_eq!(page.changes[1].change["operation"], "event.append");
    }

    #[test]
    fn replica_apply_is_idempotent_and_persists_cursor() {
        let mut source = MemoryEngine::new();
        let object = source
            .put_object(MemoryObject::new("notes", "1", r#"{"ok":true}"#))
            .unwrap();
        let mut source_replication =
            ReplicationService::new(&mut source, ReplicationConfig::source("source-a"));
        source_replication.record_object_upsert(&object).unwrap();
        let changes = source_replication.events(0, 100).unwrap().changes;
        let mut target = MemoryEngine::new();
        let mut target_replication =
            ReplicationService::new(&mut target, ReplicationConfig::replica("source-a"));
        let first = target_replication.apply(&changes).unwrap();
        let second = target_replication.apply(&changes).unwrap();
        assert_eq!(first.applied, 1);
        assert_eq!(second.skipped, 1);
        assert_eq!(
            target.get_object("notes", "1").unwrap().unwrap().body,
            r#"{"ok":true}"#
        );
    }

    #[test]
    fn delete_is_replicated_as_a_tombstone_and_allowlists_are_shared() {
        let mut source = MemoryEngine::new();
        let object = source
            .put_object(MemoryObject::new("allowed", "1", r#"{"ok":true}"#))
            .unwrap();
        source
            .put_object(MemoryObject::new("ignored", "1", r#"{"ok":true}"#))
            .unwrap();
        source.delete_object("allowed", "1").unwrap();
        let mut source_replication = ReplicationService::new(
            &mut source,
            ReplicationConfig {
                source_id: "native-a".into(),
                role: ReplicationRole::Source,
                collections: vec!["allowed".into()],
            },
        );
        source_replication.record_object_upsert(&object).unwrap();
        source_replication
            .record_object_delete("allowed", "1")
            .unwrap();
        let changes = source_replication.events(0, 100).unwrap().changes;

        let mut target = MemoryEngine::new();
        let mut target_replication = ReplicationService::new(
            &mut target,
            ReplicationConfig {
                source_id: "native-a".into(),
                role: ReplicationRole::Replica,
                collections: vec!["allowed".into()],
            },
        );
        let result = target_replication.apply(&changes).unwrap();
        assert_eq!(result.applied, 2);
        assert!(
            target
                .get_object(REPLICATION_TOMBSTONE_COLLECTION, "native-a:allowed:1")
                .unwrap()
                .is_some()
        );
        assert!(target.get_object("ignored", "1").unwrap().is_none());
    }

    #[test]
    fn source_rejects_replica_apply_and_snapshot_round_trips() {
        let mut source = MemoryEngine::new();
        let object = source
            .put_object(MemoryObject::new("notes", "1", r#"{"ok":true}"#))
            .unwrap();
        let mut source_replication =
            ReplicationService::new(&mut source, ReplicationConfig::source("native-a"));
        source_replication.record_object_upsert(&object).unwrap();
        let snapshot = source_replication.snapshot().unwrap();

        let mut target = MemoryEngine::new();
        let mut target_replication =
            ReplicationService::new(&mut target, ReplicationConfig::replica("native-a"));
        let result = target_replication.apply_snapshot(&snapshot, true).unwrap();
        assert_eq!(result.applied, 1);
        assert_eq!(result.last_applied_cursor, snapshot.cursor);

        drop(target_replication);
        let mut source_role =
            ReplicationService::new(&mut target, ReplicationConfig::source("native-a"));
        assert!(matches!(
            source_role.apply(&[]),
            Err(ThingdError::InvalidInput(_))
        ));
    }

    #[cfg(feature = "persistent")]
    #[test]
    fn persistent_replication_feed_cursor_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        {
            let mut engine = PersistentEngine::open(directory.path()).unwrap();
            let object = engine
                .put_object(MemoryObject::new("notes", "1", r#"{"ok":true}"#))
                .unwrap();
            let mut replication =
                ReplicationService::new(&mut engine, ReplicationConfig::source("persistent-a"));
            replication.record_object_upsert(&object).unwrap();
            assert_eq!(replication.events(0, 100).unwrap().next, 1);
        }
        let mut reopened = PersistentEngine::open(directory.path()).unwrap();
        let mut replication =
            ReplicationService::new(&mut reopened, ReplicationConfig::source("persistent-a"));
        let page = replication.events(0, 100).unwrap();
        assert_eq!(page.changes.len(), 1);
        assert_eq!(page.changes[0].cursor, 1);
    }
}
