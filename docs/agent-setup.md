# Agent Setup — thingd as your AI agent's memory

Three ways to give your AI agent (Cursor, Claude Desktop, or ChatGPT) a persistent
memory store with search, events, and queues.

## Which mode for which agent

| Agent | Local stdio MCP | Remote Streamable HTTP MCP |
|---|---|---|
| Cursor | ✅ recommended | ✅ |
| Claude Desktop | ✅ recommended | ✅ |
| Antigravity IDE | ✅ | ✅ |
| ChatGPT | Not directly | ✅ custom MCP app; availability depends on plan and workspace settings |

---

## 1. Local stdio (Cursor / Claude Desktop)

Follow the [5-minute quickstart](quickstart.md) for step-by-step Cursor and
Claude Desktop setup. The install, config, and verification steps are identical.

**TL;DR:**

```bash
npx thingd install
```

Your agent can then call the Node MCP server's 49 `thing_*` tools (search, objects, events, queues,
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
export THINGD_AUTH_TOKEN="$(openssl rand -hex 32)"
docker run -d \
  --name thingd \
  -p 127.0.0.1:8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN \
  -e THINGD_ALLOW_UNAUTHENTICATED=false \
  sayanmohsin/thingd
```

The host publishes the port only on loopback; the server listens on all
interfaces inside the container and requires the configured bearer token. The
server is available at `http://localhost:8757/mcp`.

### Cursor HTTP MCP config

In **Cursor Settings → Features → MCP → + Add New MCP Tool**:

| Field | Value |
|---|---|
| Name | `thingd` |
| Type | `url` |
| URL | `http://localhost:8757/mcp` |

Configure Cursor to send `Authorization: Bearer <your-token>` using its MCP
headers setting. Keep the token in a local secret store and do not put it in
the MCP URL or commit it to configuration files.

### Docker Compose (shared store between app + agent)

```yaml
# docker-compose.yml
services:
  thingd:
    image: sayanmohsin/thingd
    ports:
      - "127.0.0.1:8757:8757"
    volumes:
      - thingd-data:/data
    environment:
      THINGD_AUTH_TOKEN: ${THINGD_AUTH_TOKEN:?Set a strong token}
      THINGD_ALLOW_UNAUTHENTICATED: "false"

  your-app:
    build: .
    ports:
      - "3000:3000"
    environment:
      THINGD_URL: http://thingd:8757
      THINGD_AUTH_TOKEN: ${THINGD_AUTH_TOKEN:?Set a strong token}
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
  -H "Authorization: Bearer $THINGD_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

---

## 3. ChatGPT custom MCP app

ChatGPT connects to MCP servers through custom apps. Developer-mode access and
write-capable MCP support depend on your ChatGPT plan, workspace settings, and
administrator permissions. See [OpenAI's current developer-mode and MCP apps
guide](https://help.openai.com/en/articles/12584461-developer-mode-and-mcp-apps-in-chatgpt)
for availability and current setup steps.

For an externally reachable server, configure its HTTPS `/mcp` URL and bearer
authentication in a custom app, scan its tools, and test the app before
publishing it to a workspace. Select the app in a chat when you want to use it;
available actions depend on the app's permissions and workspace settings.

### Local or private thingd server

ChatGPT does not connect directly to a local MCP server. OpenAI documents
[Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels) for connecting private or
developer-machine MCP servers to supported OpenAI products without exposing
the server publicly. Check that guide for current product support and setup.

### Configure a public HTTPS endpoint

```text
# deploy/proxy/Caddyfile
example.com {
  reverse_proxy /mcp* localhost:8757
  reverse_proxy /healthz localhost:8757
}
```

```bash
export THINGD_AUTH_TOKEN="$(openssl rand -hex 32)"
docker run -d \
  --name thingd \
  -p 127.0.0.1:8757:8757 \
  -v thingd-data:/data \
  -e THINGD_AUTH_TOKEN \
  -e THINGD_ALLOW_UNAUTHENTICATED=false \
  sayanmohsin/thingd
```

Configure the reverse proxy to reach `127.0.0.1:8757` and publish only its
HTTPS endpoint.
Then follow OpenAI's current custom-app setup flow with
`https://example.com/mcp` and the bearer token. Avoid placing the token in the
URL or source control.

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
