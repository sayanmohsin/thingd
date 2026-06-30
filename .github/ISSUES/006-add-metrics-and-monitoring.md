Title: Add metrics and monitoring (Prometheus)

Description:
The MCP should expose operational metrics to monitor health and performance: event stream depths, queue lengths, search index lag, batch op latencies, error rates.

Suggested metrics:
- `thingd_events_total{stream}`
- `thingd_event_latency_seconds` (histogram)
- `thingd_queue_depth{collection,bookId}`
- `thingd_search_index_lag_seconds` (gauge)
- `thingd_request_errors_total{endpoint}`

Tests:
- Add smoke tests that assert metrics endpoints respond and metrics increase after actions.

Priority: Medium
Labels: infra, observability
