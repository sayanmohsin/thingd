#!/usr/bin/env bash
set -euo pipefail

IMAGE="${MEMORYD_DOCKER_IMAGE:-memoryd:local}"
CONTAINER="${MEMORYD_DOCKER_CONTAINER:-memoryd-smoke}"
PORT="${MEMORYD_DOCKER_PORT:-18757}"
TOKEN="${MEMORYD_AUTH_TOKEN:-memoryd-smoke-token}"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}

trap cleanup EXIT

docker build -t "$IMAGE" .
cleanup

docker run -d \
  --name "$CONTAINER" \
  -p "$PORT:8757" \
  -e MEMORYD_AUTH_TOKEN="$TOKEN" \
  "$IMAGE" >/dev/null

ready=0
for _ in {1..30}; do
  if curl -fsS "http://127.0.0.1:$PORT/healthz" >/dev/null 2>&1; then
    ready=1
    break
  fi

  sleep 1
done

if [[ "$ready" != "1" ]]; then
  echo "memoryd container did not become healthy on port $PORT" >&2
  docker logs "$CONTAINER" >&2 || true
  exit 1
fi

curl -fsS "http://127.0.0.1:$PORT/healthz"
printf "\n"
curl -fsS "http://127.0.0.1:$PORT/cluster/status"
printf "\n"

MEMORYD_MCP_URL="http://127.0.0.1:$PORT/mcp" \
MEMORYD_AUTH_TOKEN="$TOKEN" \
node scripts/smoke-mcp-http.mjs
