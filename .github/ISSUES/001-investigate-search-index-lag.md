Title: Investigate search index lag

Description:
Observed a delay between object deletions/updates and search index reflecting those changes (eventual consistency). This causes brief periods where `search()` still returns deleted objects.

Steps to reproduce:
1. Create several objects (e.g., batch put).
2. Delete a subset using batch delete.
3. Immediately run `search()` for deleted IDs.

Observed:
- `deleteBatch` returns success and `get()` returns null for deleted IDs, but `search()` still shows the deleted objects for a short time.

Expected:
- Search should reflect deletions within an acceptable SLA, or the system should expose the consistency window.

Suggested tests:
- Measure median and P95 latency for search index updates after writes and deletes.
- Add integration test asserting search eventually (within X seconds) returns no results for deleted IDs.

Suggested fixes:
- Improve index propagation path or provide a sync API to wait-for-index-consistency for tests.
- Document eventual consistency guarantees and recommend patterns for callers.

Priority: Medium
Labels: bug, performance, test
