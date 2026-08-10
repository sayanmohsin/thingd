# Replication API

Thingd replication synchronizes one authoritative source instance to one target
instance. Either endpoint may be local, self-hosted, or hosted by a provider
such as thingd.cloud. The protocol has no cloud-specific tenancy or billing
concepts.

## Change feed

`GET /v1/replication/events?after=<cursor>&limit=<n>` returns changes from the
source's durable replication stream. The target should persist `next` only after
the apply request succeeds.

Supported operations are `object.upsert`, `object.delete`, and `event.append`.
Internal `__thingd` collections and queue operations are not synchronized.

## Embedded native callers

Embedded Rust applications use the public `thingd::ReplicationService` with a
`MemoryEngine`, `PersistentEngine`, or another `ThingStore`. Its `events`,
`snapshot`, `apply`, `apply_snapshot`, `status`, and `conflicts` methods use
the same durable stream, cursor, allowlist, provenance, tombstone, and
quarantine semantics as the HTTP endpoints. Native applications should call
the typed `record_object_upsert`, `record_object_delete`, and
`record_event_append` methods immediately after successful source mutations.

## Apply changes

`POST /v1/replication/apply` accepts a change batch on an instance configured as
a replica:

```json
{ "changes": [/* feed items */] }
```

Applying changes must succeed before advancing the source cursor. A source
instance rejects this endpoint so a relationship cannot accidentally become
multi-master.

## Status

`GET /v1/replication/status` reports the resolved source/provider identity,
configured role, latest source cursor, last applied cursor, and the number of
quarantined conflicts. `GET /v1/replication/conflicts` returns durable
quarantine records for operator review.

## Snapshot and recovery

`GET /v1/replication/snapshot` returns the source ID, latest cursor, supported
objects, and application events needed to bootstrap a replica after a stale
cursor response. The target applies it with `POST /v1/replication/snapshot`:

```json
{
  "sourceId": "thingd-a",
  "snapshot": { "sourceId": "thingd-a", "cursor": 42, "objects": [], "events": [] },
  "replace": false
}
```

`replace: true` is a destructive rebuild and must be explicitly confirmed by
the operator. Snapshot apply preserves source metadata, writes provenance and
tombstone records, and advances the target checkpoint only after successful
application. Application events are replayed with their idempotency keys.

Cloud targets additionally require an explicit instance selection, an enabled
replica policy, an allowed source ID, and an operator confirmation header.
