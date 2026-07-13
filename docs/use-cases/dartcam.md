---
title: "Screenshot OCR Pipeline"
description: "A queue-based pipeline that OCRs screenshots and makes all text searchable through thingd."
tags: ["ocr", "queues", "search", "pipeline"]
icon: "camera"
repo: "https://github.com/sayanmohsin/dartcam"
---

## The problem

I take screenshots throughout the day — documentation, error messages, UI mockups.
The information in them is trapped in images. I can't search across screenshots
without looking at each one manually.

What I wanted: take a screenshot, have it OCR'd automatically, and be able to
search across all captured text.

## How dartcam uses thingd

dartcam uses thingd's **queues** for the processing pipeline (screenshot → OCR →
store) and **full-text search** for querying results.

```
┌──────────┐   push   ┌──────────┐   claim   ┌──────────┐
│ Capture  │ ───────→ │  thingd  │ ───────→ │  Worker  │
│ script   │          │  queue   │          │  (OCR)   │
└──────────┘          └──────────┘          └────┬─────┘
                                                  │ put
                                                  ▼
                                              ┌──────────┐
                                              │  thingd  │
                                              │  objects │
                                              └────┬─────┘
                                                    │ search
                                              ┌──────▼──────┐
                                              │  MCP tools  │
                                              │ thing_search │
                                              └─────────────┘
```

### Queuing a screenshot for processing

When a screenshot is captured, a job is pushed to the OCR queue. If OCR fails
(e.g., bad image), the job retries automatically with `nack`.

```typescript
await db.queue("ocr").push(
  { imagePath: "/tmp/screenshot-123.png", capturedAt: Date.now() },
  { idempotencyKey: `ocr:screenshot-123`, maxAttempts: 3 }
);
```

### Worker: claim, process, store

The worker claims jobs, runs OCR, and stores the result.

```typescript
const job = await db.queue("ocr").claim({ leaseMs: 30_000 });
if (job) {
  try {
    const text = await runTesseractOcr(job.payload.imagePath);
    await db.put("screenshots", {
      id: job.payload.imagePath.replace("/tmp/", ""),
      text,
      capturedAt: job.payload.capturedAt,
      ocrCompletedAt: Date.now(),
    });
    await db.queue("ocr").ack(job.id);
  } catch (err) {
    await db.queue("ocr").nack(job.id, {
      delayMs: 5_000,
      error: err.message,
    });
  }
}
```

### Searching captured text

thingd indexes the OCR'd text with FTS5, so you can search across all your
screenshots.

```bash
# via MCP
thing_search --query "error message" --collections screenshots

# via the CLI
thingd search "error message"
```

## Why thingd?

The queue with leases and retries handles the async pipeline without needing
Redis or SQS. The search is built-in — no separate Elasticsearch instance.
And the MCP tools mean agents can query screenshot text directly.

The full project is at [github.com/sayanmohsin/dartcam](https://github.com/sayanmohsin/dartcam).
