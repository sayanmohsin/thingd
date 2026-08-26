#!/usr/bin/env bash
set -euo pipefail

IMAGE="${THINGD_DOCKER_IMAGE:-thingd:local}"
CONTAINER="${THINGD_DOCKER_CONTAINER:-thingd-smoke}"
PORT="${THINGD_DOCKER_PORT:-18757}"
TOKEN="${THINGD_AUTH_TOKEN:-thingd-smoke-token}"
PLATFORM="${THINGD_DOCKER_PLATFORM:-}"
RUN_MCP_SMOKE="${THINGD_SMOKE_MCP:-1}"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}

trap cleanup EXIT

fail_endpoint() {
  local endpoint="$1"
  echo "thingd endpoint check failed: $endpoint" >&2
  docker logs "$CONTAINER" >&2 || true
  exit 1
}

build_args=()
if [[ -n "$PLATFORM" ]]; then
  build_args+=(--platform "$PLATFORM")
fi
docker build "${build_args[@]}" -f docker-context/Dockerfile -t "$IMAGE" docker-context
entrypoint="$(docker image inspect "$IMAGE" --format '{{json .Config.Entrypoint}}')"
if [[ "$entrypoint" != '["/thingd-server"]' ]]; then
  echo "unexpected Docker entrypoint: $entrypoint" >&2
  exit 1
fi
cleanup

# Published-port validation requires the server to listen on the container
# network interface, while the token keeps the non-loopback bind safe.
docker run -d \
  --name "$CONTAINER" \
  -p "$PORT:8757" \
  -e THINGD_HOST=0.0.0.0 \
  -e THINGD_AUTH_TOKEN="$TOKEN" \
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
  fail_endpoint "/healthz (startup)"
fi

curl -fsS "http://127.0.0.1:$PORT/healthz" || fail_endpoint "/healthz"
printf "\n"
curl -fsS "http://127.0.0.1:$PORT/ready" || fail_endpoint "/ready"
printf "\n"

if curl -fsS "http://127.0.0.1:$PORT/cluster/status" >/dev/null 2>&1; then
  fail_endpoint "/cluster/status (unauthenticated request was accepted)"
fi

curl -fsS \
  -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:$PORT/cluster/status" || fail_endpoint "/cluster/status (authenticated)"
printf "\n"

if [[ "$RUN_MCP_SMOKE" == "1" ]]; then
  if ! THINGD_MCP_URL="http://127.0.0.1:$PORT/mcp" \
    THINGD_AUTH_TOKEN="$TOKEN" \
    node scripts/smoke-mcp-http.mjs; then
    fail_endpoint "/mcp (authenticated)"
  fi
else
  echo "Skipping MCP smoke test (THINGD_SMOKE_MCP=$RUN_MCP_SMOKE); run pnpm smoke:docker locally for MCP validation."
fi
