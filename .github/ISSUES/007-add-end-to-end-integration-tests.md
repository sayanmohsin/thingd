Title: Add end-to-end integration tests for core workflows

Description:
Create CI-level integration tests that exercise real workflows end-to-end using an in-memory ThingD instance or test harness.

Workflows to test:
- Checkout → Return → Queue fulfillment
- Concurrent updates to same object
- Batch put/delete and subsequent search index consistency
- Event replay and idempotency

Implementation:
- Use `createMemoryThingD()` helper used in other tests.
- Run in CI as a separate job with small time budgets.

Priority: High
Labels: test, ci
