# Why Agents Use thingd

This document explains the agent leverage story in plain terms. For integration
steps see [agent-implementation-guide.md](./agent-implementation-guide.md). For
patterns see [agent-patterns.md](./agent-patterns.md). For build order see
[roadmap.md](./roadmap.md).

## The gap

Chat agents are strong at reasoning in one session and weak at:

- remembering canonical facts across conversations
- avoiding duplicate work without a shared store
- running work later or on retry
- explaining what changed and when
- handing off to a future agent run or automation

`thingd` targets that gap with **durable, structured, tool-addressable state** on
the machine — not a smarter model.

## What creates leverage

### MCP tools during work

Agents call `thing_search`, `thing_get`, `thing_put`, and queue tools without
you editing files. That is **operating on memory**, not summarizing chat.

### Objects and collections

Stable ids (`user-002`, `ord-001`) and named collections beat freeform notes.
Agents retrieve and update predictable shapes.

### Events

Append-only streams answer “what happened?” and support audit. MCP writes can
land in `__thingd:mcp:audit` so tool mutations are traceable.

### Queues

Background and delayed work: embed, notify, retry, compact. Combined with
`delayMs`, this supports scheduler-style patterns; see
[agent-patterns.md](./agent-patterns.md).

### One layer for app and agent

Apps using the Node SDK and agents using MCP can share the same store when
deployment is aligned (embedded native or sidecar).

## Mental model

```txt
objects   → current truth (RAM)
events    → history (log)
queues    → todo + timers (inbox)
MCP tools → syscalls
```

Chat is the CPU; `thingd` is persistence and coordination.

## Honest limits

- Agents must be configured to call MCP tools.
- Search is basic until [Phase 3 Search-A](./roadmap.md#phase-3--search-a-agent-memory) ships.
- Not semantic memory by default; vectors and graph links are later phases.
- Schedulers need an external heartbeat (Automation, cron, `/loop`) — thingd
  stores and queues work; it does not replace a clock.

## When to use it

**Good fit:** long-running agent projects, background agent tasks, shared app +
agent memory, audit of agent writes.

**Skip for now:** one-shot tasks, repo-only workflows where git + markdown is
enough, teams that already expose Postgres + workers via MCP.

## Quickstart pointer

1. `thingd install` — Cursor / Claude MCP config
2. `thingd mcp --driver native` — persistent `~/.thingd/data.db`
3. Put records → search → enqueue → claim/ack

Full patterns: [agent-patterns.md](./agent-patterns.md).
