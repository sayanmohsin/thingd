Title: Fix transient `get()` anomaly where `get()` returns null but `search()` finds object

Description:
During concurrent tests a `get(collection,id)` returned `null` for an object while `search()` returned the object. This appears transient but needs root cause analysis.

Steps to reproduce:
1. Rapidly create/update an object from multiple actors.
2. Immediately call `get()` and `search()` for that ID repeatedly.
3. Observe intermittent `get()` returning null while `search()` returns the object.

Observed:
- Search showed the object; `get()` returned null in at least one run.

Expected:
- `get()` and `search()` should be consistent for existence checks or documented if not.

Suggested tests:
- Add a tight loop consistency test comparing `get()` vs `search()` under concurrent writes.
- Capture timing info (createdAt/updatedAt) and RPC traces if available.

Suggested fixes:
- Fix read-path race conditions or surface a "read-after-write" helper for clients.
- Add a small retry/backoff in the SDK `get()` for short windows if needed.

Priority: High
Labels: bug, reliability, test
