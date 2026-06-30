Title: Add optimistic locking / CAS support

Description:
The MCP currently uses last-write-wins semantics for concurrent updates. This can lead to lost updates for clients that expect compare-and-set (CAS) semantics.

Use case:
- Two actors read an object at v=1, both update and write back. Last write overwrites earlier changes.

Suggested changes:
- Add optional `expectedVersion` or `ifMatch` parameter to `put()` to enforce optimistic locking.
- Return a clear error code for version mismatch (e.g., 409 Conflict).

Tests & reproduction:
1. Read object (v1) from two clients.
2. Client A writes updated object with expectedVersion=1 (succeeds).
3. Client B attempts write with expectedVersion=1 (should fail with version mismatch).

Priority: High
Labels: enhancement, api, reliability
