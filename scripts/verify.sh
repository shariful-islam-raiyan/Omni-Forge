#!/usr/bin/env bash
# Runs the same checks CI runs: formatting, lints, the full test suite, and
# an end-to-end build of the shipped example. Intended to be run locally
# before opening a PR.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo test"
cargo test --workspace

echo "==> omni doctor"
cargo run -q -p omni-cli --manifest-path Cargo.toml -- doctor || true

echo "==> building examples/hello-web"
(
  cd examples/hello-web
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build web
  test -f dist/web/index.html
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/hello-backend (requires a Python 3 toolchain)"
(
  cd examples/hello-backend
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build backend
  test -f dist/backend/output.txt
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/hello-fullstack (both targets, requires a Python 3 toolchain)"
(
  cd examples/hello-fullstack
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build all
  test -f dist/web/index.html
  test -f dist/web/assets/api-client.js
  test -f dist/backend/app/service.py
  test -f dist/backend/output.txt
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/navbar-demo (component composition)"
(
  cd examples/navbar-demo
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build web
  test -f dist/web/index.html
  grep -q 'class="navbar"' dist/web/index.html
  grep -q 'class="page"' dist/web/index.html
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "All checks passed."
