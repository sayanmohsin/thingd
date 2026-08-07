# @thingd/cli

[![npm](https://img.shields.io/npm/v/@thingd/cli?label=@thingd/cli&logo=npm&color=ff6a00)](https://www.npmjs.com/package/@thingd/cli)

CLI, interactive TUI dashboard, and MCP server for [thingd](https://github.com/sayanmohsin/thingd) — a fast object-first data engine for applications and AI agents.

## Install

```bash
npm install -g @thingd/cli
```

## Usage

```bash
# Start the MCP server (for AI agents — Cursor, Claude Desktop)
thingd mcp

# Start the HTTP MCP server + dashboard
thingd mcp-http

# Optional encrypted native storage; inject the key, do not put it in MCP config
THINGD_ENCRYPTION_KEY=<64-hex-characters> thingd mcp --driver native

# Offline migration or key rotation
THINGD_ENCRYPTION_SOURCE_KEY=<old-key> \
THINGD_ENCRYPTION_DESTINATION_KEY=<new-key> \
thingd db reencrypt --source ./old-db --destination ./new-db

# Open the TUI dashboard
thingd dashboard

# Manage objects
thingd objects list <collection>
thingd objects get <collection> <id>
thingd objects put <collection> <id> --data '{"text":"hello"}'
thingd objects delete <collection> <id>
thingd objects put-batch <collection> --file <path>
thingd objects delete-batch <collection> <id1> [id2] ...

# Events
thingd events append <stream> <type> --text "event data"
thingd events list [stream]

# Queues
thingd queues push <queue> --payload '{"key":"value"}'
thingd queues claim <queue>
thingd queues ack <queue> <jobId>
thingd queues nack <queue> <jobId>

# Links
thingd links create <fromRef> <linkType> <toRef>
thingd links neighbors <reference>

# Utilities
thingd search <query>
thingd status
thingd doctor
thingd install
thingd export --collection <name> --out <path>
thingd import --collection <name> --in <path>
thingd snapshot create --out <path>
thingd snapshot restore --in <path>
```

Full reference: [docs/cli-reference.md](https://github.com/sayanmohsin/thingd/blob/main/docs/cli-reference.md)
