#!/usr/bin/env bash
set -euo pipefail

: "${EXPECTED_CLIENT_IP:?set EXPECTED_CLIENT_IP to the real client IP, e.g. 192.168.1.253}"
: "${PROXY_HOST:=192.168.1.103}"
: "${PROXY_PORT:=3128}"
: "${ELASTIC_URL:=http://localhost:9200}"
: "${ELASTIC_USER:=elastic}"
: "${ELASTIC_PASS:=changeme-elastic}"
: "${INDEX_PATTERN:=traffic-events-*}"
: "${INGEST_URL:=http://localhost:19100}"
: "${INGEST_SECRET:=changeme-ingest}"
: "${WAIT_SECONDS:=120}"
: "${VERIFY_TRUSTED_XFF_PROMOTION:=0}"

probe_token="od-e2e-$(date +%s)"
probe_url="http://example.com/?${probe_token}=1"
deadline=$(( $(date +%s) + WAIT_SECONDS ))

echo "[E2E] Production-Linux proxy verification"
echo "[E2E] Expected client IP: ${EXPECTED_CLIENT_IP}"
echo "[E2E] Proxy endpoint: ${PROXY_HOST}:${PROXY_PORT}"
echo "[E2E] Probe URL token: ${probe_token}"
echo
echo "Run this command on the client machine now:"
echo "curl -v -x http://${PROXY_HOST}:${PROXY_PORT} '${probe_url}'"
echo

matched_line=""
while [[ $(date +%s) -lt ${deadline} ]]; do
  matched_line=$(grep "${probe_token}" data/squid-logs/access.log | tail -n 1 || true)
  if [[ -n "${matched_line}" ]]; then
    break
  fi
  sleep 2
done

if [[ -z "${matched_line}" ]]; then
  echo "[E2E] Did not observe probe token ${probe_token} in squid access log within ${WAIT_SECONDS}s" >&2
  exit 1
fi

echo "[E2E] Squid line: ${matched_line}"

observed_source_ip=$(awk '{print $3}' <<<"${matched_line}")
if [[ "${observed_source_ip}" != "${EXPECTED_CLIENT_IP}" ]]; then
  echo "[E2E] Source IP mismatch. Observed ${observed_source_ip}, expected ${EXPECTED_CLIENT_IP}" >&2
  exit 1
fi

echo "[E2E] Squid source IP matches expected client IP"

sleep 5

query_payload=$(cat <<JSON
{"size":1,"sort":[{"@timestamp":{"order":"desc"}}],"query":{"bool":{"must":[{"term":{"source.ip":"${EXPECTED_CLIENT_IP}"}},{"match_phrase":{"message":"${probe_token}"}}]}}}
JSON
)

es_hit_count=$(curl -sS -u "${ELASTIC_USER}:${ELASTIC_PASS}" \
  -H 'Content-Type: application/json' \
  -d "${query_payload}" \
  "${ELASTIC_URL}/${INDEX_PATTERN}/_search" | jq -r '.hits.total.value')

if [[ "${es_hit_count}" -lt 1 ]]; then
  echo "[E2E] Elasticsearch did not index probe with source.ip=${EXPECTED_CLIENT_IP}" >&2
  exit 1
fi

echo "[E2E] Elasticsearch contains real-client probe event"

trusted_xff_event=$(cat <<JSON
{"@timestamp":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","message":"1775659000.000 10 ${EXPECTED_CLIENT_IP} TCP_TUNNEL/200 39 CONNECT www.bing.com:443 - HIER_DIRECT/150.171.28.16 - \"203.0.113.9, ${EXPECTED_CLIENT_IP}\"","event":{"original":"1775659000.000 10 ${EXPECTED_CLIENT_IP} TCP_TUNNEL/200 39 CONNECT www.bing.com:443 - HIER_DIRECT/150.171.28.16 - \"203.0.113.9, ${EXPECTED_CLIENT_IP}\""},"od":{"service":"squid","source":"squid","environment":"dev","integration_test":"proxy-production-linux-e2e"}}
JSON
)

curl -sS -o /dev/null -w "%{http_code}" \
  -X POST "${INGEST_URL}/ingest/filebeat" \
  -H 'Content-Type: application/json' \
  -H "X-Filebeat-Secret: ${INGEST_SECRET}" \
  -d "${trusted_xff_event}" | grep -Eq '200|202'

sleep 2

xff_query=$(cat <<JSON
{"size":1,"sort":[{"@timestamp":{"order":"desc"}}],"query":{"term":{"od.integration_test":"proxy-production-linux-e2e"}}}
JSON
)

observed_client_ip=$(curl -sS -u "${ELASTIC_USER}:${ELASTIC_PASS}" \
  -H 'Content-Type: application/json' \
  -d "${xff_query}" \
  "${ELASTIC_URL}/${INDEX_PATTERN}/_search" | jq -r '.hits.hits[0]._source.client.ip')

if [[ "${VERIFY_TRUSTED_XFF_PROMOTION}" = "1" ]]; then
  if [[ "${observed_client_ip}" != "203.0.113.9" ]]; then
    echo "[E2E] Trusted XFF promotion failed: expected client.ip=203.0.113.9, got ${observed_client_ip}" >&2
    exit 1
  fi
  echo "[E2E] Trusted XFF promotion verified (client.ip=203.0.113.9)"
else
  if [[ "${observed_client_ip}" != "${EXPECTED_CLIENT_IP}" ]]; then
    echo "[E2E] Default anti-spoof expectation failed: expected client.ip=${EXPECTED_CLIENT_IP}, got ${observed_client_ip}" >&2
    exit 1
  fi
  echo "[E2E] Default anti-spoof verified (client.ip remains source.ip)"
fi

echo "[E2E] PASS"
