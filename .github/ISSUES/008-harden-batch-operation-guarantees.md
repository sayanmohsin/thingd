Title: Harden batch operation guarantees and add compensating actions

Description:
Batch operations (putBatch/deleteBatch) should have well-documented atomicity/consistency semantics. Partial failures need clear compensating actions.

Suggested improvements:
- Return detailed per-item status for batch operations.
- Add a transactional mode or a compensation/log for partial failures.
- Add tests simulating partial I/O failures.

Priority: Medium
Labels: enhancement, reliability, test
