#!/usr/bin/env bash
set -euo pipefail

: "${BASE_URL:=http://localhost:19000}"
: "${ADMIN_TOKEN:=changeme-admin}"

fail() {
  echo "[security] $1" >&2
  exit 1
}

echo "[security] Verifying unauthenticated request is rejected"
unauth_status=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/api/v1/overrides")
[[ "$unauth_status" == "401" ]] || fail "expected 401 for unauthenticated overrides list (got $unauth_status)"

echo "[security] Verifying authenticated read succeeds"
auth_status=$(curl -s -o /dev/null -w "%{http_code}" -H "X-Admin-Token: ${ADMIN_TOKEN}" "${BASE_URL}/api/v1/overrides")
[[ "$auth_status" == "200" ]] || fail "expected 200 for authenticated overrides list (got $auth_status)"

echo "[security] Testing payload validation for override scopes"
payload='{"scope_type":"domain;DROP","scope_value":"example.com","action":"allow","status":"active"}'
invalid_status=$(curl -s -o /dev/null -w "%{http_code}" -H "Content-Type: application/json" -d "$payload" "${BASE_URL}/api/v1/overrides")
[[ "$invalid_status" == "401" ]] || echo "(expected 401 because request lacks token)"
invalid_status=$(curl -s -o /dev/null -w "%{http_code}" -H "Content-Type: application/json" -H "X-Admin-Token: ${ADMIN_TOKEN}" -d "$payload" "${BASE_URL}/api/v1/overrides")
[[ "$invalid_status" == "400" ]] || fail "expected 400 when submitting override with invalid scope (got $invalid_status)"

echo "[security] Verifying refresh token endpoint rejects invalid token"
refresh_status=$(curl -s -o /dev/null -w "%{http_code}" \
  -H "Content-Type: application/json" \
  -d '{"refresh_token":"invalid-refresh-token"}' \
  "${BASE_URL}/api/v1/auth/refresh")
[[ "$refresh_status" == "401" || "$refresh_status" == "400" ]] || fail "expected 401/400 for invalid refresh token (got $refresh_status)"

echo "[security] Verifying change-password route rejects non-user principals"
change_status=$(curl -s -o /dev/null -w "%{http_code}" \
  -H "Content-Type: application/json" \
  -H "X-Admin-Token: ${ADMIN_TOKEN}" \
  -d '{"current_password":"x","new_password":"y"}' \
  "${BASE_URL}/api/v1/auth/change-password")
[[ "$change_status" == "403" || "$change_status" == "401" ]] || fail "expected 403/401 for non-user password change attempt (got $change_status)"

echo "[security] AuthZ smoke checks passed"
