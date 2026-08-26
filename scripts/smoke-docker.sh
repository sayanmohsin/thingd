#!/usr/bin/env bash
set -euo pipefail

IMAGE="${THINGD_DOCKER_IMAGE:-thingd:local}"
CONTAINER="${THINGD_DOCKER_CONTAINER:-thingd-smoke}"
PORT="${THINGD_DOCKER_PORT:-18757}"
TOKEN="${THINGD_AUTH_TOKEN:-thingd-smoke-token}"
PLATFORM="${THINGD_DOCKER_PLATFORM:-}"

cleanup() {
  docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}

trap cleanup EXIT

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
  echo "thingd container did not become healthy on port $PORT" >&2
  docker logs "$CONTAINER" >&2 || true
  exit 1
fi

if ! curl -fsS "http://127.0.0.1:$PORT/ready" >/dev/null 2>&1; then
  echo "thingd container did not become ready on port $PORT" >&2
  docker logs "$CONTAINER" >&2 || true
  exit 1
fi

curl -fsS "http://127.0.0.1:$PORT/healthz"
printf "\n"
curl -fsS "http://127.0.0.1:$PORT/cluster/status"
printf "\n"

THINGD_MCP_URL="http://127.0.0.1:$PORT/mcp" \
THINGD_AUTH_TOKEN="$TOKEN" \
node scripts/smoke-mcp-http.mjs
