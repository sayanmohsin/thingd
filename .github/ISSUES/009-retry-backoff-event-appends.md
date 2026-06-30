Title: Retry and backoff for event appends with idempotency

Description:
Transient failures when appending to event streams should be retried with exponential backoff and idempotency guarantees to avoid duplicate events.

Suggested changes:
- Add client SDK support for idempotent event append IDs.
- Implement server-side deduplication window for event IDs.
- Retry with exponential backoff on transient errors.

Tests:
- Simulate transient network failures and verify no duplicate events in stream.

Priority: Medium
Labels: enhancement, reliability
