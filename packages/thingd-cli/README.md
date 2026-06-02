# thingd-cli

[![npm version](https://badge.fury.io/js/thingd-cli.svg)](https://www.npmjs.com/package/thingd-cli)

Command-line interface, Interactive TUI Dashboard, and Model Context Protocol (MCP) server for [thingd](https://www.npmjs.com/package/thingd).

This package provides the unified `thingd` binary, which serves as a persistent stdio/HTTP MCP server for AI editors (Cursor, Claude Desktop), an interactive terminal UI dashboard for database operations, and a scriptable JSON utility to inspect and manage local or remote databases.

## Installation

Install globally via npm to use the `thingd` command anywhere:

```bash
npm install -g thingd-cli
```

Or run it on the fly using `npx`:

```bash
npx thingd-cli
```

## Quick Start: Zero-Config MCP Setup

The easiest way to integrate `thingd` with your AI editors (Claude Desktop and Cursor) is to run the automated setup command:

```bash
thingd install
```

This command will:
- Auto-detect the absolute path of your active `node` binary and global `thingd` script.
- Automatically locate or create your persistent local database at `~/.thingd/data.db`.
- **For Claude Desktop**: Automatically configure the stdio MCP server in your `claude_desktop_config.json` (macOS).
- **For Cursor**: Print the exact, copy-pasteable JSON block to paste into your Cursor MCP settings.

## Two Modes of Operation

The CLI is designed to support both human operators and automated scripts through two distinct modes:

1. **Interactive Dashboard (TUI)**: Run `thingd` without any arguments to launch a beautiful, interactive terminal UI. Perfect for exploring collections, editing objects, and managing queues manually.
2. **Non-Interactive CLI**: Run `thingd <command>` for one-off operations. This mode intentionally emits JSON so it is easy to use from shell scripts, CI/CD pipelines, and AI-agent workflows.

---

## 1. Interactive Dashboard (TUI)

To start the interactive dashboard, simply run:
```bash
thingd
```

### Connection Mode
Upon launch, you will be prompted to select a connection driver:
- `Memory`: An ephemeral, in-memory instance (great for quick testing).
- `Native`: Connects directly to a local SQLite `.db` file (requires a file path).
- `Cloud`: Connects to a remote ThingD cluster (requires a URL and an optional Bearer Token).

### Keyboard Shortcuts
Once connected, the TUI provides a dual-pane layout: a navigation sidebar on the left and a detailed viewer/editor on the right.

| Shortcut | Action |
|----------|--------|
| `↑` / `↓` (or `k` / `j`) | Navigate the tree vertically |
| `←` / `→` (or `h` / `l`) | Expand or collapse folders (Collections, Streams, Queues) |
| `Enter` | Expand node or open a folder |
| `c` | **Create** a new Object, Event, or push a Queue job |
| `e` | **Edit** an Object or retry a Queue job |
| `d` | **Delete** an Object or Ack/Resolve a Queue job |
| `/` (or `f`) | **Global Semantic Search** across the database |
| `i` | **Info**: View Connection Info & Cloud Cluster Status |
| `r` | **Refresh** data from the database |
| `s` | **Switch** database connections |
| `q` | **Quit** |

---

## 2. Non-Interactive CLI

For scripts and automation, you can bypass the TUI entirely. 

### Common Options
You can configure the connection via environment variables or CLI flags:

```txt
--url <url>          remote thingd URL. Defaults to THINGD_URL
--auth-token <tok>   remote bearer token. Defaults to THINGD_AUTH_TOKEN
--path <path>        local database path. Defaults to THINGD_PATH or ~/.thingd/data.db
--driver <driver>    memory, native, or cloud
--pretty             pretty-print JSON output
--limit <n>          result limit for search and list commands
```

### Claude Desktop & MCP Integration

The `thingd` CLI has a built-in `mcp` subcommand that exposes an MCP server over `stdio`. This allows Claude Desktop to securely connect to your `thingd` database.

**Connecting to a Local Database:**
To let Claude read and write to a local `thingd.db` file, add this to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "thingd-local": {
      "command": "thingd",
      "args": ["mcp", "--path", "/absolute/path/to/thingd.db", "--driver", "native"]
    }
  }
}
```

**Bridging Claude to the Cloud:**
Claude Desktop natively only supports local `stdio` servers. However, `thingd mcp` can act as a bridge! If you provide a `--url`, the CLI will launch locally but seamlessly proxy all of Claude's requests to your remote cluster:

```json
{
  "mcpServers": {
    "thingd-cloud": {
      "command": "thingd",
      "args": ["mcp", "--url", "https://your-thingd-cloud.com/mcp", "--auth-token", "your-secret-token"]
    }
  }
}
```

**Running as a Streamable HTTP Server:**
If you want to run `thingd` as a standalone Streamable HTTP MCP server (for example, to connect remote agents or ChatGPT), you can spin it up directly using `mcp-http`:

```bash
thingd mcp-http --path ./thingd.db --driver native --port 8757 --auth-token change-me
```

### Command Reference

**System & Diagnostics**
```bash
thingd doctor                 # Run Node, native binding, and remote connectivity diagnostics
thingd metrics                # Get total database counts (objects, events, activeJobs, deadJobs)
thingd status                 # Check cluster health (requires --url)
thingd tools                  # List available MCP tools (requires --url)
thingd bench rust --smoke     # Run Rust SQLite engine smoke benchmarks
thingd bench rust --count 500 # Run Rust SQLite engine benchmarks with specific run count
```

**Discovery & Collections**
```bash
thingd collections list       # List all active collections
thingd streams list           # List all active stream names (alias for events streams)
thingd events streams         # List all active event streams
thingd queues list-all        # List all active queue names
```

**Search**
```bash
thingd search "my query" [--collection <name>] [--limit <n>]
```

**Objects**
```bash
thingd objects list decisions                  # List all objects inside a collection
thingd objects put decisions core --text "msg" # Put object with plain text content
thingd objects put decisions core --data '{"a":1}' # Put object with arbitrary JSON data
thingd objects get decisions core              # Fetch a specific object by ID
thingd objects delete decisions core           # Delete an object by ID
```

**Events**
```bash
thingd events append project:thingd decision.made --text "Picked the CLI shape."
thingd events list project:thingd
```

**Queues**
```bash
thingd queues stats embed                      # View queue statistics (ready, leased, dead jobs)
thingd queues push embed --payload '{"object":"docs/readme"}'
thingd queues claim embed
thingd queues ack embed <jobId>
thingd queues nack embed <jobId> --error "failed to fetch" --delay-ms 5000
thingd queues list embed
thingd queues dead embed
```

**Data Movement & Snapshots**
```bash
# Export & Import Data with optional recursive secret/email/key redaction
thingd export --collection decisions --out decisions.jsonl [--redact]
thingd export --events [--stream name] --out events.jsonl [--redact custom_secret]
thingd import --collection decisions_restored --in decisions.jsonl

# Snapshot & Restore full database state
thingd snapshot create --out snapshot.thingd.json
thingd snapshot restore --in snapshot.thingd.json
```
