# Cursor Agent Memory Examples

This directory contains fully functional, runnable Node.js examples and configuration files demonstrating how to equip your AI agents and subagents with durable long-term memory, scheduling, and blackboard task coordination using `thingd`.

---

## 🔌 Cursor & Claude MCP Registration

Equip Cursor or Claude Desktop with `thingd` tools by registering the MCP server.

### Cursor Registration
1. Open Cursor **Settings** (`Cmd + ,` on macOS).
2. Go to **Features** -> **MCP**.
3. Click **+ Add New MCP Tool**:
   - **Name**: `thingd`
   - **Type**: `command`
   - **Command**: `thingd mcp --driver native`
4. Click **Save**.

### Claude Desktop Registration
Add the following configuration block to your Claude Desktop configuration file (typically `~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):
```json
{
  "mcpServers": {
    "thingd": {
      "command": "thingd",
      "args": ["mcp", "--driver", "native"]
    }
  }
}
```

---

## 🚀 Runnable Examples

This folder contains two runnable scripts. Make sure to run them from your terminal.

### 1. `quickstart.js`
Demonstrates opening the database using the SDK, seeding documents, full-text FTS5 search (with stemming), and custom metadata filters.

Run the quickstart:
```bash
node quickstart.js
```

### 2. `scheduler-heartbeat.js`
Demonstrates **Pattern 2 (Objects + Queue + External Heartbeat)**. It showcases an atomic task scheduler: registering schedules, pushing due tasks into the queue with idempotency keys, claiming them via leases, and advancing recurring schedules.

Run the scheduler:
```bash
node scheduler-heartbeat.js
```

---

## 🤖 Cursor Rules (`.cursorrules`)

The `.cursorrules` file in this directory serves as a tailored instruction set for AI subagents. By copying `.cursorrules` to your project root, you teach agents:
- **Search-Before-Put**: How to verify and search existing memories before writing, minimizing duplicate and bloated records.
- **Transactional Auditing**: Emitting audit records using metadata parameters on writes.
- **Blackboard Coordination**: Claiming, leasing, and resolving background queue tasks safely without conflicts.
