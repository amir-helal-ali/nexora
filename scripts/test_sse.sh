#!/bin/bash
# Test SSE live event streaming end-to-end.
set -e
cd /home/z/my-project/nexora

. "$HOME/.cargo/env"

# Start gateway
./target/release/gateway-demo 127.0.0.1:8080 &
GW_PID=$!
sleep 2

# Login
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/auth/login -H "Content-Type: application/json" -d '{"username":"admin","password":"admin123"}' | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])")
echo "Got token: ${TOKEN:0:30}..."

# Start SSE listener in background (max 6 seconds)
timeout 6 curl -sN http://127.0.0.1:8080/api/core/events/stream -H "Authorization: Bearer $TOKEN" > /tmp/sse_output.txt 2>&1 &
SSE_PID=$!
sleep 1

# Publish 3 events while SSE is listening
echo "Publishing 3 events..."
for i in 1 2 3; do
  curl -s -X POST http://127.0.0.1:8080/api/core/events -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -d "{\"name\":\"live.test\",\"payload\":\"event-$i\"}" > /dev/null
  sleep 0.3
done

# Wait for SSE timeout
wait $SSE_PID 2>/dev/null || true

echo ""
echo "=== SSE Output (first 40 lines) ==="
head -40 /tmp/sse_output.txt

kill $GW_PID 2>/dev/null || true
sleep 1
echo "--- done ---"
