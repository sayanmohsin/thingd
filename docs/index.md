---
layout: home
title: thingd — Open-Source Rust Data Engine for AI Agents
description: thingd is an open-source Rust data engine for AI agents. Object-shaped storage, durable queues, event streams, full-text search, and 27 MCP tools — all in one binary. Built by Sayan Mohsin.

hero:
  name: thingd
  text: Open-source Rust data engine for AI agents
  tagline: Object-shaped storage, durable queues, event streams, full-text search, graph links, and 27 MCP-native tools — all in one static binary. No stitching together separate infrastructure.
  actions:
    - theme: brand
      text: Get started →
      link: /quickstart
    - theme: alt
      text: GitHub
      link: https://github.com/sayanmohsin/thingd
    - theme: alt
      text: Docker
      link: https://hub.docker.com/r/sayanmohsin/thingd

features:
  - icon: 🗄️
    title: Object Store
    details: Versioned JSON records in named collections. No schema. Put, get, delete, list. Auto-versioning.
  - icon: 📋
    title: Durable Queues
    details: At-least-once with leases, retries, delays, idempotency keys, and dead-letter queue.
  - icon: 📡
    title: Event Streams
    details: Append-only timelines with auto-incrementing sequences. Built for audit and replication.
  - icon: 🔍
    title: Full-text Search
    details: SQLite FTS5 with Porter stemming, metadata filters, and recency-weighted BM25 ranking.
  - icon: 🤖
    title: MCP-native
    details: 27 built-in MCP tools. AI agents read, write, search, and process jobs without custom code.
  - icon: ⚡
    title: Multi-mode
    details: Embedded, sidecar, or cluster. In-memory or persistent SQLite. Same API everywhere.
---
