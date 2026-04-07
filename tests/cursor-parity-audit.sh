#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT_DIR=${OUT_DIR:-"$ROOT_DIR/tests/artifacts/cursor-parity-audit/$(date +%Y%m%d%H%M%S)"}
mkdir -p "$OUT_DIR"

grep -R --line-number "page_size\|PageOptions\|Paged<" "$ROOT_DIR/services/admin-api/src" --include='*.rs' >"$OUT_DIR/admin-api-page-patterns.txt" || true
grep -R --line-number "cursor\|limit\|CursorPaged" "$ROOT_DIR/services/admin-api/src" --include='*.rs' >"$OUT_DIR/admin-api-cursor-patterns.txt" || true

cat >"$OUT_DIR/README.txt" <<EOF
Cursor parity audit artifacts

Generated at: $(date -Iseconds)

Files:
- admin-api-page-patterns.txt
- admin-api-cursor-patterns.txt
EOF

printf 'cursor parity audit artifacts: %s\n' "$OUT_DIR"
