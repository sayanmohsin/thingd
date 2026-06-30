Title: Queue auto-recovery and cleanup

Description:
Wait queues can become stale or stuck (fulfilled entries not cleaned, positions not rebalanced) after partial failures or manual edits.

Problems observed:
- `wait_queue` entries can remain in `fulfilled` or `waiting` state while corresponding checkouts are inconsistent.

Suggested fixes:
- Implement a background reconciler that verifies queue entries against `checkouts` and `books` and fixes or alerts on inconsistencies.
- Rebalance positions after deletions/fulfillments.

Tests:
- Simulate partial failures: mark queue entry fulfilled without creating checkout; verify reconciler creates missing checkout or marks queue for manual review.

Priority: Medium
Labels: enhancement, reliability, ops
