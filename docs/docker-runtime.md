# Docker Runtime

`memoryd` can be packaged as a container that runs the MCP server over
Streamable HTTP.

This is the first remote-capable runtime shape. It is intended for local
experiments, self-hosting, and the future `memoryd-cloud` gateway.

## Build

```bash
docker build -t memoryd:local .
```

## Run

```bash
docker run --rm \
  -p 8757:8757 \
  -v memoryd-data:/data \
  -e MEMORYD_AUTH_TOKEN=change-me \
  memoryd:local
```

The container starts:

```txt
node packages/memoryd-mcp/dist/http-cli.js
```

Default container environment:

```txt
MEMORYD_PATH=/data/memoryd.db
MEMORYD_DRIVER=native
MEMORYD_HOST=0.0.0.0
MEMORYD_PORT=8757
```

## Endpoints

```txt
GET  /healthz
POST /mcp
```

When `MEMORYD_AUTH_TOKEN` is set, `/mcp` requires:

```txt
Authorization: Bearer <token>
```

## Health Check

```bash
curl http://127.0.0.1:8757/healthz
```

## MCP Client URL

Local URL:

```txt
http://127.0.0.1:8757/mcp
```

For ChatGPT or cloud-hosted agents, localhost is not enough. The MCP endpoint
must be available at a public HTTPS URL with authentication.

```txt
ChatGPT / hosted agent
  -> https://your-domain.example/mcp
  -> memoryd container
  -> /data/memoryd.db
```

Do not expose a tokenless MCP endpoint to the public internet.

## Current Limitations

- no TLS termination inside the container
- no OAuth
- no multi-tenant routing
- no migrations
- no production prebuild matrix
- no audit events for MCP writes yet

Put TLS, domains, and public exposure behind a proper reverse proxy or hosted
gateway.
