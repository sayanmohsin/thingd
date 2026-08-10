#!/usr/bin/env bash
# Test tenant isolation with MultiTenant mode.
#
# Starts the engine, writes the same object ID to two different tenants,
# and verifies each tenant sees only their own data.
#
# Usage:
#   ./scripts/test-tenant-isolation.sh [--engine-binary path/to/thingd-server]

set -uo pipefail

ENGINE_BIN="${1:-cargo run --}"
TMPDIR=$(mktemp -d /tmp/thingd-tenant-test-XXXXXX)
DB_PREFIX="$TMPDIR/tenants/"
CONFIG_FILE="$TMPDIR/config.yaml"
ENGINE_PORT=18757
ENGINE_PID=""
PASS=0
FAIL=0

ALICE_TOKEN="alice-token-for-tenant-isolation"
BOB_TOKEN="bob-token-for-tenant-isolation"

cat > "$CONFIG_FILE" <<EOF
auth:
  tenant_tokens:
    alice: "$ALICE_TOKEN"
    bob: "$BOB_TOKEN"
tenant:
  mode: multi-tenant
  database_prefix: "$DB_PREFIX"
EOF

cleanup() {
  [ -n "$ENGINE_PID" ] && kill "$ENGINE_PID" 2>/dev/null && wait "$ENGINE_PID" 2>/dev/null || true
  rm -rf "$TMPDIR"
  echo "---"
  echo "Results: $PASS passed, $FAIL failed"
  [ "$FAIL" -eq 0 ]
}

trap cleanup EXIT

# ── Start engine ──────────────────────────────────────────────
echo "==> Starting thingd-server in multi-tenant mode on port $ENGINE_PORT..."
if [[ "$ENGINE_BIN" == "cargo run --" ]]; then
  THINGD_HOST=127.0.0.1 \
  THINGD_PORT=$ENGINE_PORT \
  THINGD_PATH="$TMPDIR/shared/thingd.db" \
  THINGD_CONFIG="$CONFIG_FILE" \
  THINGD_TENANT_MODE=multi-tenant \
  THINGD_TENANT_DB_PREFIX="$DB_PREFIX" \
  cargo run &
elif [[ -x "$ENGINE_BIN" ]]; then
  THINGD_HOST=127.0.0.1 \
  THINGD_PORT=$ENGINE_PORT \
  THINGD_PATH="$TMPDIR/shared/thingd.db" \
  THINGD_CONFIG="$CONFIG_FILE" \
  THINGD_TENANT_MODE=multi-tenant \
  THINGD_TENANT_DB_PREFIX="$DB_PREFIX" \
  "$ENGINE_BIN" &
else
  echo "ERROR: $ENGINE_BIN not found and not a cargo command"
  exit 1
fi
ENGINE_PID=$!

# Wait for engine to be ready
for i in $(seq 1 20); do
  if curl -sf "http://127.0.0.1:$ENGINE_PORT/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done
if ! curl -sf "http://127.0.0.1:$ENGINE_PORT/healthz" >/dev/null 2>&1; then
  echo "ERROR: Engine failed to start"
  exit 1
fi
echo "       Engine ready"

# ── Run tests ──────────────────────────────────────────────────
MCP_URL="http://127.0.0.1:$ENGINE_PORT/mcp"
CT="Content-Type: application/json"

mcpcall() {
  local tenant="$1"
  shift
  local token="$ALICE_TOKEN"
  [ "$tenant" = "bob" ] && token="$BOB_TOKEN"
  curl -sf -X POST "$MCP_URL" \
    -H "X-Tenant-Id: $tenant" \
    -H "Authorization: Bearer $token" \
    -H "$CT" \
    -d "$@" 2>/dev/null
}

echo ""
echo "==> Test 1: Write same object ID to two tenants"
mcpcall "alice" '{"method":"tools/call","params":{"name":"thing_put","arguments":{"collection":"notes","object":{"id":"msg","text":"alices data"}}}}' >/dev/null
mcpcall "bob"   '{"method":"tools/call","params":{"name":"thing_put","arguments":{"collection":"notes","object":{"id":"msg","text":"bobs data"}}}}' >/dev/null
echo "       Written"

echo ""
echo "==> Test 2: Each tenant sees only their own data"
alice_get=$(mcpcall "alice" '{"method":"tools/call","params":{"name":"thing_get","arguments":{"collection":"notes","id":"msg"}}}')
bob_get=$(mcpcall "bob"   '{"method":"tools/call","params":{"name":"thing_get","arguments":{"collection":"notes","id":"msg"}}}')

if echo "$alice_get" | grep -q "alices data"; then
  echo "  ✅ alice sees her data"
  ((PASS++))
else
  echo "  ❌ alice sees wrong data: $alice_get"
  ((FAIL++))
fi

if echo "$bob_get" | grep -q "bobs data"; then
  echo "  ✅ bob sees his data"
  ((PASS++))
else
  echo "  ❌ bob sees wrong data: $bob_get"
  ((FAIL++))
fi

echo ""
echo "==> Test 3: Files are physically separate"
if [ -e "$DB_PREFIX/alice/thingd.db" ]; then
  echo "  ✅ alice's DB exists: $DB_PREFIX/alice/thingd.db"
  ((PASS++))
else
  echo "  ❌ alice's DB not found"
  ((FAIL++))
fi

if [ -e "$DB_PREFIX/bob/thingd.db" ]; then
  echo "  ✅ bob's DB exists: $DB_PREFIX/bob/thingd.db"
  ((PASS++))
else
  echo "  ❌ bob's DB not found"
  ((FAIL++))
fi

echo ""
echo "==> Test 4: Missing tenant header returns error"
no_header=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$MCP_URL" \
  -H "$CT" \
  -d '{"method":"tools/call","params":{"name":"thing_get","arguments":{"collection":"notes","id":"msg"}}}')
if [ "$no_header" = "400" ]; then
  echo "  ✅ Missing header returns 400"
  ((PASS++))
else
  echo "  ❌ Missing header returned $no_header (expected 400)"
  ((FAIL++))
fi

echo ""
echo "==> Test 5: Path traversal in tenant ID is rejected"
traversal=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$MCP_URL" \
  -H "X-Tenant-Id: ../../etc/passwd" \
  -H "$CT" \
  -d '{"method":"tools/call","params":{"name":"thing_get","arguments":{"collection":"notes","id":"msg"}}}')
if [ "$traversal" = "401" ]; then
  echo "  ✅ Path traversal rejected (401)"
  ((PASS++))
else
  echo "  ❌ Path traversal returned $traversal (expected 401)"
  ((FAIL++))
fi

echo ""
echo "==> Test 6: Default TenantMode::Single works (no header required)"
# Start a second engine without multi-tenant mode
TMPDIR2=$(mktemp -d /tmp/thingd-tenant-single-XXXXXX)
SINGLE_PORT=18758

THINGD_HOST=127.0.0.1 \
THINGD_PORT="$SINGLE_PORT" \
THINGD_PATH="$TMPDIR2/thingd.db" \
THINGD_TENANT_MODE=single \
"$ENGINE_BIN" &
SINGLE_PID=$!

for i in $(seq 1 20); do
  if curl -sf "http://127.0.0.1:$SINGLE_PORT/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.5
done

# Should work without tenant header
single_ok=$(curl -s -o /dev/null -w "%{http_code}" -X POST "http://127.0.0.1:$SINGLE_PORT/mcp" \
  -H "$CT" \
  -d '{"method":"tools/call","params":{"name":"ping"}}')
kill "$SINGLE_PID" 2>/dev/null || true
wait "$SINGLE_PID" 2>/dev/null || true
rm -rf "$TMPDIR2"

if [ "$single_ok" = "200" ]; then
  echo "  ✅ Single mode works without tenant header"
  ((PASS++))
else
  echo "  ❌ Single mode failed ($single_ok)"
  ((FAIL++))
fi
