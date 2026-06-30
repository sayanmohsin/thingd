# Docs Update: `thingd mcp connect`

## 1. `docs/cli-reference.md`

### Change A — update `thingd install` line

**Old (line 24):**
```
thingd install              zero-config setup for Cursor / Claude Desktop
```

**New:**
```
thingd install [--raw] [--claude] [--cursor] [--antigravity]    zero-config local setup for Cursor / Claude Desktop / Antigravity IDE
```

### Change B — add `thingd mcp connect` line

**Insert after line 25 (after `thingd doctor` line):**
```
thingd mcp connect            configure agents for cloud MCP (requires thingd cloud login)
```

---

## 2. `docs/mcp-server.md`

### Change — add "Cloud MCP Connect" section after "Zero-Config Setup"

**Insert after line 68 (after "See the 5-minute quickstart..."):**

```md
### Cloud MCP Connect

If you use thingd Cloud, generate agent config pointing at your hosted MCP endpoint:

```bash
thingd cloud login          # authenticate
thingd mcp connect          # pick project/instance → writes config
```

This command:
1. Fetches your projects and instances from thingd Cloud
2. Pre-fills the MCP URL from your instance's endpoint
3. Pre-fills the auth token from your login session
4. Lets you edit URL and token before writing
5. Writes to Claude Desktop, Antigravity IDE, or prints Cursor-compatible JSON

Requires `thingd cloud login` first.
```

---

## 3. `docs/agent-setup.md`

### Change A — update the "Which mode for which agent" table

**Old (lines 8-13):**
```
| Agent | stdio MCP | HTTP MCP (Docker) | HTTP MCP (remote) |
|---|---|---|---|
| Cursor | ✅ recommended | ✅ | ❌ localhost only |
| Claude Desktop | ✅ recommended | ✅ | ❌ localhost only |
| ChatGPT | ❌ | ❌ | ✅ needs public URL |
```

**New:**
```
| Agent | stdio MCP | HTTP MCP (Docker) | Cloud MCP (thingd Cloud) | HTTP MCP (remote) |
|---|---|---|---|---|
| Cursor | ✅ recommended | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| Claude Desktop | ✅ recommended | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| Antigravity IDE | ✅ | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| ChatGPT | ❌ | ❌ | ❌ | ✅ needs public URL |
```

### Change B — add section 1b after line 31

**Insert after "For the full list.":**

```md
### 1b. Cloud MCP (Cursor / Claude Desktop / Antigravity IDE)

For thingd Cloud users, generate agent config for your hosted MCP endpoint:

```bash
thingd cloud login       # one-time auth
thingd mcp connect       # pick project, pick instance, write config
```

You'll be prompted to:
1. Select a project
2. Select an instance
3. Review the pre-filled MCP URL and auth token (editable)
4. Choose a destination: Claude Desktop, Antigravity IDE, or print for Cursor

The config uses `url` + `Authorization` header instead of `command`/`args`.
```

---

## 4. `docs/quickstart.md`

### Change — add cloud quickstart after manual Claude Desktop entry

**Insert after line 56 (after the manual Claude Desktop JSON block):**

```md
### Cloud setup (thingd Cloud)

```bash
npx thingd cloud login
npx thingd mcp connect
```

Follow the prompts to select your project and instance. The config is written to Claude Desktop (on macOS) or printed for Cursor.
```

---

## 5. `README.md`

### Change — update MCP-native access section

**Old (lines 426-429):**
```
Run the automatic zero-config setup for Claude Desktop and Cursor:

```bash
# Installs/updates Claude Desktop config automatically and prints Cursor configuration
thingd install
```
```

**New:**
```
Run automatic setup for local or cloud MCP:

```bash
# Local — configures Claude Desktop / Cursor / Antigravity IDE for a local sidecar
thingd install

# Cloud — configures agents for your thingd Cloud MCP endpoint (requires thingd cloud login)
thingd mcp connect
```
```

---

## 6. `docs/faq.md`

### Change — add `mcp connect` reference in MCP section

**Insert after line 161 (after "What MCP tools are exposed?" section heading) or at the end of the "MCP / AI-agent integration" subsection:**

```md
### How do I configure agents for thingd Cloud?

Run `thingd mcp connect` after logging in with `thingd cloud login`. It fetches your
projects and instances, pre-fills the MCP URL and auth token, and writes the config
to Claude Desktop (on macOS), Antigravity IDE, or prints it for Cursor.
```

---

## 7. `docs/why-agents.md`

### Change — update quickstart section

**Old (lines 101-102):**
```
1. `thingd install` — configure Cursor / Claude Desktop
2. `thingd mcp --driver native` — persistent `~/.thingd/data.db`
```

**New:**
```
1. `thingd install` — local agent config; or `thingd mcp connect` for cloud
2. `thingd mcp --driver native` — persistent `~/.thingd/data.db`
```

---

## 8. `docs/blog-drafts.md`

### Change — update mentions of `npx thingd install`

**Line 102:** Replace or add alongside `npx thingd install`:
```md
npx thingd install          # Local agent setup
npx thingd mcp connect      # Cloud agent setup (after thingd cloud login)
```

**Line 146:** Same change.

**Line 292:** Same change.

---

## 9. `docs/reddit-drafts.md`

### Change — update line 137

**Old:**
```
  npx thingd install
```

**New:**
```
  npx thingd install       # Local setup
  npx thingd mcp connect   # Cloud setup (after thingd cloud login)
```

---

## 10. `AGENTS.md`

### Change — update audit checklist

Add to the "Doc audit after every change" checklist (around line 127):

- `docs/cli-reference.md` — CLI commands and flags (add `thingd mcp connect`, `install --antigravity`)
- `docs/agent-setup.md` — cloud MCP setup path
- `docs/mcp-server.md` — cloud connect section
- `docs/quickstart.md` — cloud setup path
