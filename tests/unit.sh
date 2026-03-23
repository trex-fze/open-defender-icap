#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
pushd "$ROOT_DIR" >/dev/null

echo "[unit] Running Rust workspace tests"
cargo test --workspace

echo "[unit] Running web-admin unit tests"
pushd web-admin >/dev/null
npm install >/dev/null
npm run test >/dev/null
popd >/dev/null

echo "All unit tests completed successfully"
popd >/dev/null
