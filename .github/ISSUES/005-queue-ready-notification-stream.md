Title: Add `queue.ready` notification stream for queue fulfillment

Description:
When a queue entry is fulfilled (book returned → checkout created), the system should publish a `queue.ready` event on a notifications stream so downstream services can notify users.

Suggested implementation:
- Append a `queue.ready` event to a `notifications` stream with payload: `{subscriberId, bookId, queueId, pickupBy}`.
- Ensure idempotency for retries.

Tests:
- Fulfill a queue entry and verify the `notifications` stream receives exactly one `queue.ready` event.

Priority: Low
Labels: feature, integrations
