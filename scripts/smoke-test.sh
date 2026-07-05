#!/usr/bin/env bash
set -e

cd /home/z/my-project/nexora
RUST_LOG=warn ./target/release/gateway-demo 127.0.0.1:18080 > /tmp/gateway.log 2>&1 &
GW_PID=$!
sleep 3

LOGIN=$(curl -s -X POST http://127.0.0.1:18080/api/auth/login -H 'Content-Type: application/json' -d '{"username":"admin","password":"admin123"}')
TOKEN=$(echo "$LOGIN" | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))")

echo "===== FINAL SMOKE TEST ====="
echo ""
echo "1. Health:       $(curl -s http://127.0.0.1:18080/api/health)"
echo "2. Login:        Token length: $(echo -n $TOKEN | wc -c) bytes"
echo "3. Ping:         $(curl -s -X POST http://127.0.0.1:18080/api/core/ping -H "Authorization: Bearer $TOKEN")"
echo "4. Publish:      $(curl -s -X POST http://127.0.0.1:18080/api/core/events -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"name":"smoke.test","payload":"ok"}')"
echo "5. Marketplace:  $(curl -s http://127.0.0.1:18080/api/marketplace/packages -H "Authorization: Bearer $TOKEN" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("count=" + str(d.get("count","?")))')"
echo "6. Cluster:      $(curl -s http://127.0.0.1:18080/api/cluster/nodes -H "Authorization: Bearer $TOKEN" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("count=" + str(d.get("count","?")))')"
echo "7. Workflows:    $(curl -s http://127.0.0.1:18080/api/workflows -H "Authorization: Bearer $TOKEN" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("count=" + str(d.get("count","?")))')"
echo "8. Notifs:       $(curl -s http://127.0.0.1:18080/api/notifications -H "Authorization: Bearer $TOKEN" | python3 -c 'import sys,json; d=json.load(sys.stdin); print("count=" + str(d.get("count","?")))')"
echo "9. Billing:      $(curl -s http://127.0.0.1:18080/api/billing/stats -H "Authorization: Bearer $TOKEN" | head -c 120)"
echo ""
echo "10. GraphQL:     $(curl -s -X POST http://127.0.0.1:18080/api/graphql -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' -d '{"query":"{ health { healthy modulesTotal eventsPublished uptimeSeconds } }"}' | head -c 250)"
echo ""
echo "11. OpenAPI:     $(curl -s http://127.0.0.1:18080/api/openapi.json | python3 -c 'import sys,json; s=json.load(sys.stdin); print(str(len(s.get("paths",{}))) + " routes, v" + str(s.get("info",{}).get("version")))')"
echo "12. Auth fail:   HTTP $(curl -s -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:18080/api/core/ping)"
echo "13. Wrong pwd:   HTTP $(curl -s -o /dev/null -w '%{http_code}' -X POST http://127.0.0.1:18080/api/auth/login -H 'Content-Type: application/json' -d '{"username":"admin","password":"WRONG"}')"
echo ""
echo "===== ALL TESTS DONE ====="
kill $GW_PID 2>/dev/null
wait $GW_PID 2>/dev/null
