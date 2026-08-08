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

`GET /v1/replication/status` reports the source ID, configured role, latest
source cursor, and number of retained changes.
