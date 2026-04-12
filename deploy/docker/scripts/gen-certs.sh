#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CERT_DIR="$ROOT_DIR/squid/certs"
WEB_ADMIN_CERT_DIR="$ROOT_DIR/web-admin/certs"

mkdir -p "$CERT_DIR"
mkdir -p "$WEB_ADMIN_CERT_DIR"

echo "Generating Squid CA certificate..."
openssl req \
    -new \
    -newkey rsa:4096 \
    -days 825 \
    -nodes \
    -x509 \
    -subj "/CN=OpenDefender Squid CA" \
    -keyout "$CERT_DIR/ca.key" \
    -out "$CERT_DIR/ca.pem"

echo "Generating Squid server certificate signed by the CA..."
openssl req \
    -new \
    -newkey rsa:4096 \
    -nodes \
    -subj "/CN=squid.local" \
    -keyout "$CERT_DIR/server.key" \
    -out "$CERT_DIR/server.csr"

openssl x509 \
    -req \
    -in "$CERT_DIR/server.csr" \
    -CA "$CERT_DIR/ca.pem" \
    -CAkey "$CERT_DIR/ca.key" \
    -CAcreateserial \
    -out "$CERT_DIR/server.pem" \
    -days 825 \
    -sha256

rm -f "$CERT_DIR/server.csr" "$CERT_DIR/ca.srl"

echo "Certificates written to $CERT_DIR"
echo "Import $CERT_DIR/ca.pem into Squid clients to trust the proxy."

echo "Generating web-admin self-signed TLS certificate..."
openssl req \
    -new \
    -newkey rsa:4096 \
    -days 825 \
    -nodes \
    -x509 \
    -subj "/CN=localhost" \
    -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
    -keyout "$WEB_ADMIN_CERT_DIR/web-admin.key" \
    -out "$WEB_ADMIN_CERT_DIR/web-admin.pem"

echo "Web-admin TLS cert written to $WEB_ADMIN_CERT_DIR"
echo "Import $WEB_ADMIN_CERT_DIR/web-admin.pem into your browser/OS trust store for warning-free https://localhost:19001 access."
