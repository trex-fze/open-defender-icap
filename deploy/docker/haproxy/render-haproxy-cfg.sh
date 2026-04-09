#!/bin/sh
set -eu

output_cfg="${1:-/tmp/haproxy.generated.cfg}"

allowed_client_cidrs="${OD_SQUID_ALLOWED_CLIENT_CIDRS:-192.168.1.0/24}"
backend_host="${OD_HAPROXY_BACKEND_HOST:-squid}"
backend_port="${OD_HAPROXY_BACKEND_PORT:-3128}"
listen_port="${OD_HAPROXY_LISTEN_PORT:-3128}"

acl_lines=""
old_ifs="$IFS"
IFS=','
for raw_cidr in $allowed_client_cidrs; do
  cidr="$(printf '%s' "$raw_cidr" | tr -d '[:space:]')"
  if [ -z "$cidr" ]; then
    continue
  fi
  acl_lines="${acl_lines}  acl allowed_client src ${cidr}\n"
done
IFS="$old_ifs"

if [ -z "$acl_lines" ]; then
  echo "render-haproxy-cfg: no CIDRs configured in OD_SQUID_ALLOWED_CLIENT_CIDRS" >&2
  exit 1
fi

cat > "$output_cfg" <<EOF
global
  log stdout format raw local0

defaults
  log global
  mode http
  option httplog
  timeout connect 10s
  timeout client 2m
  timeout server 2m

frontend forward_proxy
  bind :${listen_port}
  mode http
$(printf '%b' "$acl_lines")
  http-request deny unless allowed_client
  http-request set-header X-Real-IP "%[src]"
  http-request set-header X-Forwarded-For "%[src]"
  http-request set-header X-Forwarded-Proto "http"
  http-request set-header X-Forwarded-Host "%[req.hdr(Host)]"
  http-request set-header X-Forwarded-Port "%[dst_port]"
  http-request set-header Forwarded "for=%[src];proto=http;host=%[req.hdr(Host)]"
  acl is_connect method CONNECT
  http-request set-header X-Forwarded-Proto "https" if is_connect
  http-request set-header Forwarded "for=%[src];proto=https;host=%[req.hdr(Host)]" if is_connect
  default_backend squid_backend

backend squid_backend
  mode http
  server squid ${backend_host}:${backend_port}
EOF
