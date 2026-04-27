#!/bin/sh
set -eu

ALLOW_INSECURE_DEV_SECRETS="${OD_ALLOW_INSECURE_DEV_SECRETS:-false}"
WARNINGS=0
ISSUES=0

lower() {
  printf "%s" "$1" | tr '[:upper:]' '[:lower:]'
}

is_true() {
  case "$(lower "$1")" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

record_violation() {
  key="$1"
  reason="$2"
  if is_true "$ALLOW_INSECURE_DEV_SECRETS"; then
    WARNINGS=$((WARNINGS + 1))
    printf 'warning: insecure development mode enabled via OD_ALLOW_INSECURE_DEV_SECRETS=true; allowing unsafe value for %s (%s)\n' "$key" "$reason" >&2
  else
    ISSUES=$((ISSUES + 1))
    printf 'error: unsafe secret detected for %s (%s)\n' "$key" "$reason" >&2
  fi
}

check_secret() {
  key="$1"
  min_len="$2"
  required="$3"
  eval "value=\${$key-}"

  if [ -z "${value:-}" ]; then
    if [ "$required" = "required" ]; then
      record_violation "$key" "missing required value"
    fi
    return
  fi

  trimmed=$(printf "%s" "$value" | awk '{$1=$1};1')
  length=$(printf "%s" "$trimmed" | wc -c | tr -d ' ')
  if [ "$length" -lt "$min_len" ]; then
    record_violation "$key" "value shorter than minimum length ($min_len)"
  fi

  lowered=$(lower "$trimmed")
  case "$lowered" in
    *changeme*|defender|password|secret|admin|test|default|example|sample|placeholder|insecure|dummy)
      record_violation "$key" "matches blocked default/test pattern"
      ;;
  esac

  case "$lowered" in
    changeme-admin|changeme-ingest|changeme-elastic|changeme-local-admin-password|changeme-local-jwt-secret|changeme-local-jwt-secret-min-32-chars|aaeaawvsyxn0awmvakliywjhl29klxn0ywnrok1muujovnlnu19tsmndt0z6ovzmexg)
      record_violation "$key" "matches repository example value"
      ;;
  esac
}

check_url_auth() {
  key="$1"
  mode="$2"
  eval "value=\${$key-}"

  if [ -z "${value:-}" ]; then
    return
  fi

  case "$mode" in
    userpass)
      if ! printf "%s" "$value" | grep -Eq '^[a-zA-Z][a-zA-Z0-9+.-]*://[^:/@]+:[^@]+@'; then
        record_violation "$key" "URL must include username and password credentials"
      fi
      ;;
    pass)
      if ! printf "%s" "$value" | grep -Eq '^[a-zA-Z][a-zA-Z0-9+.-]*://[^@]*:[^@]+@'; then
        record_violation "$key" "URL must include password credentials"
      fi
      ;;
  esac
}

check_secret POSTGRES_PASSWORD 12 required
check_secret ELASTIC_PASSWORD 12 required
check_secret ELASTICSEARCH_SERVICEACCOUNTTOKEN 24 required
check_secret OD_ADMIN_TOKEN 16 required
check_secret OD_POLICY_ADMIN_TOKEN 16 required
check_secret OD_LOCAL_AUTH_JWT_SECRET 32 required
check_secret OD_DEFAULT_ADMIN_PASSWORD 12 required
check_secret OD_FILEBEAT_SECRET 16 required
check_secret OPENAI_API_KEY 16 optional
check_secret LLM_API_KEY 16 optional

check_url_auth OD_ADMIN_DATABASE_URL userpass
check_url_auth OD_POLICY_DATABASE_URL userpass
check_url_auth OD_TAXONOMY_DATABASE_URL userpass
check_url_auth OD_CACHE_REDIS_URL pass
check_url_auth OD_PAGE_FETCH_REDIS_URL pass

if [ "$ISSUES" -gt 0 ]; then
  printf 'secret preflight failed with %s issue(s). Set OD_ALLOW_INSECURE_DEV_SECRETS=true only for explicit development mode.\n' "$ISSUES" >&2
  exit 1
fi

if [ "$WARNINGS" -gt 0 ]; then
  printf 'secret preflight passed with %s warning(s) due to explicit development override.\n' "$WARNINGS" >&2
else
  printf 'secret preflight passed with secure values.\n'
fi
