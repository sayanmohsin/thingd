# Agent Setup — thingd as your AI agent's memory

Three ways to give your AI agent (Cursor, Claude Desktop, GPT) a persistent
memory store with search, events, and queues.

## Which mode for which agent

| Agent | stdio MCP | HTTP MCP (Docker) | Cloud MCP (thingd Cloud) | HTTP MCP (remote) |
|---|---|---|---|---|
| Cursor | ✅ recommended | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| Claude Desktop | ✅ recommended | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| Antigravity IDE | ✅ | ✅ | ✅ `thingd mcp connect` | ❌ localhost only |
| ChatGPT | ❌ | ❌ | ❌ | ✅ needs public URL |

---

## 1. Local stdio (Cursor / Claude Desktop)

Follow the [5-minute quickstart](quickstart.md) for step-by-step Cursor and
Claude Desktop setup. The install, config, and verification steps are identical.

**TL;DR:**

```bash
npx thingd install
```

Your agent can then call all 36 `thing_*` tools (search, objects, events, queues,
links, counts, aggregate, schema, NLQ, vector, discovery). See the [MCP tools reference](api-spec/mcp-tools.md)
for the full list.

### Optional encrypted local storage

For native persistent stdio MCP, inject the key into the host process before
starting the server:

```bash
export THINGD_ENCRYPTION_KEY=<64-hex-characters>
thingd mcp --driver native
```

The MCP client configuration remains unchanged. The client never receives the
key; the process must open the database successfully before it starts serving
MCP. Missing or wrong keys appear as a startup failure, not as a tool result.

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

---

## 2. Docker HTTP MCP (Cursor / any HTTP MCP client)

Run thingd as a Docker sidecar and connect your agent over HTTP.

### Start the container

```bash
docker run -d \
  --name thingd \
  -p 8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN=my-token \
  -e THINGD_ENCRYPTION_KEY=<64-hex-characters> \
  sayanmohsin/thingd
```

The server is now at `http://localhost:8757/mcp`.

### Cursor HTTP MCP config

In **Cursor Settings → Features → MCP → + Add New MCP Tool**:

| Field | Value |
|---|---|
| Name | `thingd` |
| Type | `url` |
| URL | `http://localhost:8757/mcp` |

Cursor sends the `Authorization` header automatically when you add a token in
the URL field — include it as `http://localhost:8757/mcp` and add the header
manually via Cursor's MCP headers setting, or use an MCP proxy that injects the
bearer token. Alternatively, run without auth on localhost:

```bash
docker run -d \
  --name thingd \
  -p 8757:8757 \
  -v thingd-data:/data \
  -e THINGD_ALLOW_UNAUTHENTICATED=true \
  sayanmohsin/thingd
```

> **Safety**: Without auth, the endpoint listens on `127.0.0.1` inside the
> container but Docker maps it to `0.0.0.0` on the host. Only use
> `ALLOW_UNAUTHENTICATED=true` on a single-user machine. For shared or
> production use, set `THINGD_AUTH_TOKEN` and configure Cursor to send
> `Authorization: Bearer <token>`.

### Docker Compose (shared store between app + agent)

```yaml
# docker-compose.yml
services:
  thingd:
    image: sayanmohsin/thingd
    ports:
      - "8757:8757"
    volumes:
      - thingd-data:/data
    environment:
      - THINGD_AUTH_TOKEN=my-token

  your-app:
    build: .
    ports:
      - "3000:3000"
    environment:
      - THINGD_URL=http://thingd:8757
      - THINGD_AUTH_TOKEN=my-token
    depends_on:
      - thingd

volumes:
  thingd-data:
```

Your app and your agent (Cursor/Claude) both connect to the same store.

### Verify

```bash
curl http://localhost:8757/healthz
# → OK

curl -X POST http://localhost:8757/mcp \
  -H "Authorization: Bearer my-token" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## 3. GPT / ChatGPT (remote HTTP MCP)

ChatGPT supports MCP over HTTPS. thingd's Streamable HTTP endpoint works
directly — but your local machine is not reachable from ChatGPT's servers.

### Option A: SSH tunnel (quick test)

```bash
# On your server or a public machine:
docker run -d \
  --name thingd \
  -p 8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN=strong-random-token \
  sayanmohsin/thingd

# From your local machine:
ssh -L 8757:localhost:8757 your-server
```

ChatGPT needs a public HTTPS URL, so a plain tunnel isn't enough by itself. Use
a reverse proxy with TLS.

### Option B: Reverse proxy with TLS (recommended)

```nginx
# deploy/proxy/Caddyfile
example.com {
  reverse_proxy /mcp* localhost:8757
  reverse_proxy /healthz localhost:8757
}
```

```bash
docker run -d \
  --name thingd \
  --network host \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN=strong-random-token \
  sayanmohsin/thingd
```

Your MCP endpoint: `https://example.com/mcp`

### Register with ChatGPT

1. Open ChatGPT → Settings → MCP servers
2. Add new server:
   - **Name**: `thingd`
   - **URL**: `https://example.com/mcp`
   - **Bearer token**: `strong-random-token`

ChatGPT can now call all 36 thingd tools in any conversation.

---

## Agent rules (optional but powerful)

Copy the `.cursorrules` file to your project root to teach agents the memory
conventions automatically:

```bash
cp node_modules/@thingd/cli/examples/cursor-agent-memory/.cursorrules .cursorrules
```

Or write your own system prompt for GPT / Claude:

```txt
You have access to a thingd memory store via MCP tools.

Conventions:
- Use thing_search before thing_put to avoid duplicates
- Store decisions in the "decisions" collection
- Append events to "project:<name>" streams for audit trails
- Queue background work (embedding, summarization) via thing_queue_push
- Use idempotency keys for queue jobs to ensure at-most-once processing
```

See [agent-patterns.md](./agent-patterns.md) for ready-made patterns:
scheduler, multi-agent blackboard, agent handoff, inbox, and heartbeat.

---

## Common issues

| Symptom | Fix |
|---|---|
| Cursor shows "MCP server not connected" | Ensure `thingd` CLI is on your `PATH` and run `thingd doctor` |
| Docker container exits immediately | Check `docker logs thingd` — likely a port conflict or missing volume |
| ChatGPT can't connect | The URL must be HTTPS with a valid certificate. Use Caddy or a cloud proxy |
| Agent writes don't persist after restart | Use `--driver native` (stdio) or ensure a volume is mounted (Docker) |
| "Tool not found" in agent | Agent may need to refresh tool list. Restart the conversation or reconnect MCP |

---

## Reference

- [Quickstart (5 minutes)](./quickstart.md)
- [MCP server reference](./mcp-server.md)
- [Docker runtime](./docker-runtime.md)
- [Runtime environment variables](./runtime-env.md)
- [Agent patterns](./agent-patterns.md)
- [Why agents use thingd](./why-agents.md)
- [API spec](./api-spec/)
