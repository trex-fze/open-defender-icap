#!/bin/sh
set -eu

input_conf="${1:-/etc/squid/squid.conf}"
output_conf="${2:-/tmp/squid.generated.conf}"

allowed_client_cidrs="${OD_SQUID_ALLOWED_CLIENT_CIDRS:-192.168.1.0/24}"
trusted_proxy_cidrs="${OD_TRUSTED_PROXY_CIDRS:-}"

build_acl_lines() {
  acl_name="$1"
  cidr_csv="$2"
  allow_empty="${3:-false}"
  acl_lines=""

  old_ifs="$IFS"
  IFS=','
  for raw_cidr in $cidr_csv; do
    cidr="$(printf '%s' "$raw_cidr" | tr -d '[:space:]')"
    if [ -z "$cidr" ]; then
      continue
    fi
    acl_lines="${acl_lines}acl ${acl_name} src ${cidr}\n"
  done
  IFS="$old_ifs"

  if [ -z "$acl_lines" ] && [ "$allow_empty" != "true" ]; then
    echo "render-squid-conf: no CIDRs configured for acl '${acl_name}'" >&2
    exit 1
  fi

  printf '%b' "$acl_lines"
}

allowed_acl_lines="$(build_acl_lines "localnet" "$allowed_client_cidrs")"
trusted_acl_lines="$(build_acl_lines "trusted_proxies" "$trusted_proxy_cidrs" true)"

if [ -n "$trusted_acl_lines" ]; then
  follow_xff_rules="${trusted_acl_lines}\nfollow_x_forwarded_for allow trusted_proxies\nfollow_x_forwarded_for deny all\nforwarded_for on\n"
else
  follow_xff_rules="follow_x_forwarded_for deny all\nforwarded_for on\n"
fi

awk -v allowed="$allowed_acl_lines" -v follow_xff="$follow_xff_rules" '
  {
    gsub(/__OD_SQUID_ALLOWED_CLIENT_ACLS__/, allowed)
    gsub(/__OD_SQUID_FOLLOW_XFF_RULES__/, follow_xff)
    print
  }
' "$input_conf" > "$output_conf"
