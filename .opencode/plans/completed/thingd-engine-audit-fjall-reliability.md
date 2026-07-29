# Thingd Fjall Reliability Gate

## Status

Complete

## Summary

Harden the `FjallEngine` adapter against restart, retry, secondary-index, and
multi-key consistency failures identified by the 2026-07-28 engine audit. Keep
the existing storage traits and SQLite implementation as the compatibility and
performance baseline. Do not fork Fjall or remove SQLite until this gate passes
restart, crash-interruption, transaction, index-rebuild, parity, and workload
validation.

## Problem and goals

The current Fjall adapter passes its existing unit tests, but important state
and derived indexes are only partially durable. The goals are to:

- preserve event sequence and idempotency semantics across reopen;
- ensure delayed queue jobs remain claimable;
- keep search and vector indexes synchronized with primary data;
- make related keyspace updates atomic where the contract requires it;
- align Fjall and MemoryEngine behavior;
- create evidence for the Fjall-versus-SQLite backend decision.

## Scope and non-goals

### In scope

- `FjallEngine` startup recovery and durable metadata.
- Fjall queue ready-index lifecycle.
- Tantivy open/rebuild/update/delete behavior.
- Object/vector lifecycle consistency.
- Link, object, queue, event, and batch atomicity.
- Shared adapter contract and restart/crash tests.
- Cloud handoff and roadmap gate updates already recorded in the private repo.

### Non-goals

- Replacing the public storage traits.
- Forking or rewriting Fjall.
- Removing SQLite before the gate passes.
- Introducing a new query language or changing public SDK/MCP behavior.
- Implementing HNSW/vector generation as part of this reliability work.

## Repository evidence

| Area | File | Relevant behavior |
|---|---|---|
| Trait contracts | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/store.rs` | Defines object, event, queue, link, search, aggregate, and vector semantics. |
| Reference adapter | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/in_memory.rs` | Provides expected in-memory behavior and existing unit coverage. |
| Durable adapter | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/fjall.rs:38-108` | Keeps event counters/idempotency maps in memory and initializes keyspaces. |
| Event append | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/fjall.rs:578-620` | Allocates per-stream sequences from process-local state. |
| Queue claim | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/fjall.rs:787-867` | Removes a delayed job's ready index entry when it sorts first. |
| Object/vector write | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/fjall.rs:213-243` | Writes a vector only when present; does not clear an old vector. |
| Search indexing | `/Users/sayanmohsin/Space/Programming/ancatag/thingd/crates/thingd/src/fjall.rs:1007-1084` | Adds documents on updates and deletes only object records. |
| Cloud gate | `/Users/sayanmohsin/Space/Programming/ancatag/thingd-cloud/docs/thingd/roadmap.md:545` | Phase 22 tracks Fjall migration and reliability completion criteria. |

## Requirements

### Functional requirements

1. Reopening a database must continue each stream at a sequence greater than
   its durable maximum.
2. Reopening a database must preserve event idempotency for existing keys.
3. A delayed job must not be removed from the ready index before it is
   claimable.
4. An expired lease must be requeued exactly once and remain claimable.
5. Updating an object must replace its search record rather than create a
   duplicate logical hit.
6. Deleting an event or stream must remove its search records.
7. Updating an object without a vector must remove any prior vector entry.
8. Multi-key state transitions must be atomic or have a documented recovery
   protocol that restores derived indexes.
9. Fjall and MemoryEngine must agree on duplicate pushes, protected streams,
   time bounds, vector cleanup, and result limits.
10. Existing databases must either open their search index or rebuild it from
    primary Fjall data without silently degrading behavior.

### Non-functional requirements

- Preserve existing trait and SDK/MCP contracts unless a contract mismatch is
  explicitly documented and approved.
- Avoid holding unbounded duplicate recovery state in memory when it can be
  derived from durable records.
- Keep recovery deterministic and idempotent.
- Ensure all new persistence behavior is covered by tests that reopen the same
  temporary database.
- Keep the implementation compatible with the current Rust edition, feature
  flags, and `forbid(unsafe_code)` policy.

## Proposed design

### Durable event recovery

At `FjallEngine::open`, scan event keys or maintain a dedicated durable
metadata keyspace. Reconstruct the maximum sequence per stream and the
idempotency map before returning the engine. Prefer a durable metadata design
only if it can be updated atomically with event writes; otherwise derive state
from primary events and add an optimization later.

Deletion must define sequence behavior explicitly. Deleting a stream may reset
its sequence only if the public contract permits reuse without overwriting
historical records; otherwise preserve a high-water mark in durable metadata.

### Queue ready-index lifecycle

Treat `available_at_ms` as part of claim ordering. A delayed entry must remain
indexed, be skipped without deletion, or be indexed in a structure that can
efficiently find the next available job. Stale entries for deleted or terminal
jobs should be removed and rebuilt safely. Use a bounded iterative scan rather
than unbounded recursive retries.

### Secondary-index consistency

Use stable logical document keys and delete/replace old search records on object
updates. Delete event records from search when deleting an event or stream.
Open an existing Tantivy index when present; otherwise create it. Add a rebuild
path that clears and reconstructs derived documents from Fjall primary data.

When an object write has no vector, remove its vector key. Object, vector, and
search changes should use the strongest available Fjall write-batch or
transaction boundary. If Tantivy cannot participate in the Fjall transaction,
persist a rebuildable index version/state and make recovery explicit.

### Adapter contract tests

Extract shared scenarios into a contract-oriented test module or reusable test
helpers. Run the same semantic cases against `MemoryEngine` and
`FjallEngine`; keep backend-specific performance and recovery tests separate.

## Implementation impact

| Phase | Files/modules | Change | Done when |
|---|---|---|---|
| 1. Contract confirmation | `crates/thingd/src/store.rs`, `model.rs`, API specs | Confirm sequence deletion, duplicate push, time bounds, index, and atomicity semantics. | Requirements are unambiguous and any contract changes are documented. |
| 2. Event recovery | `crates/thingd/src/fjall.rs` | Recover sequence high-water marks and idempotency state on open; define delete behavior. | Reopen tests preserve sequence and idempotency. |
| 3. Queue correctness | `crates/thingd/src/fjall.rs` | Fix delayed ready-index handling, stale-index cleanup, and overflow-safe time arithmetic. | Delayed, expired, retried, and reopened jobs behave correctly. |
| 4. Derived indexes | `crates/thingd/src/fjall.rs` | Make search/vector updates replacement-safe; open/rebuild Tantivy indexes. | Update/delete/reopen/rebuild tests return only current records. |
| 5. Atomic transitions | `crates/thingd/src/fjall.rs` | Use Fjall transaction/write-batch APIs for related keyspaces; define recovery for external Tantivy state. | Interruption tests show no unrecoverable orphaned primary/index state. |
| 6. Adapter parity | `crates/thingd/src/in_memory.rs`, `fjall.rs`, shared tests | Align documented behavior and add parity scenarios. | Shared contract suite passes on both adapters. |
| 7. Baseline and rollout | `crates/thingd/`, `thingd-cloud/docs/thingd/` | Run benchmarks against SQLite, update gate status, retain migration fallback. | Phase 22 is marked complete only with evidence. |

## Contracts and examples

### Event reopen

```text
append(stream="audit", idempotencyKey="k") -> sequence=1
drop engine; reopen same path
append(stream="audit", idempotencyKey="k") -> existing sequence=1
append(stream="audit", idempotencyKey="k2") -> sequence=2
```

### Delayed queue claim

```text
push(priority=10, availableAt=now+60s)
push(priority=0, availableAt=now)
claim() -> low-priority available job
after 60s: claim() -> delayed high-priority job
```

### Vector removal

```text
put(collection="docs", id="a", vector=[1, 0])
put(collection="docs", id="a", vector=None)
vectorSearch("docs", [1, 0]) -> no result for "a"
```

## Test and verification matrix

| Behavior | Test location | Verification |
|---|---|---|
| Event sequence survives reopen | `crates/thingd/src/fjall.rs` tests | Open, append, drop, reopen, append; assert no overwrite. |
| Idempotency survives reopen | `crates/thingd/src/fjall.rs` tests | Repeat same stream/key after reopen; assert same event. |
| Delayed job remains claimable | `crates/thingd/src/fjall.rs` tests | Claim before and after availability, including a higher-priority delayed job. |
| Search update/delete/reopen | `crates/thingd/src/fjall.rs` tests | Assert one current hit, no deleted event hit, and rebuild parity. |
| Vector removal | `crates/thingd/src/in_memory.rs`, `fjall.rs` tests | Update vector to `None`; assert search excludes object. |
| Multi-key interruption | Fjall recovery tests or temporary crash harness | Reopen after interruption; assert primary/index invariants. |
| Adapter parity | Shared engine contract tests | Run identical cases against MemoryEngine and FjallEngine. |
| Full engine validation | workspace commands | `cargo test -p thingd --all-features`; Clippy; format check. |
| Backend decision | benchmark suite | Compare realistic object/event/queue/index workloads with SQLite. |

## Risks, assumptions, and open questions

- Fjall transaction API selection must be verified against the pinned Fjall
  version before implementation.
- Tantivy is a separate storage system; full cross-system atomicity may require
  a rebuild protocol rather than one transaction.
- Existing data may already contain stale or missing derived indexes. Startup
  recovery must be safe for both clean and partially upgraded directories.
- Stream deletion and sequence reuse need an explicit compatibility decision.
- Multi-process opening of one Fjall database remains a deployment constraint;
  sidecar and cluster ownership must continue to enforce single-database
  ownership.
- The existing uncommitted dashboard and skill changes are unrelated and must
  not be included in implementation commits.

## Handoff instructions

Read this spec, `AGENTS.md`, and the current `thingd-cloud` Fjall gate before
editing. Implement phases in dependency order. Add regression tests before
changing migration status. Do not fork Fjall or remove SQLite during this
work. If transaction semantics, stream sequence reuse, or Tantivy recovery
cannot be made safe without a product decision, stop and report the blocker
instead of guessing.
