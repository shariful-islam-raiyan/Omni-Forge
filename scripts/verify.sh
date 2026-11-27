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
# OMNIFORGE_RUN_ANDROID_E2E opts into the real Gradle build in
# omni-android-adapter's own test suite (see
# docs/decisions/0011-android-gradle-adapter.md) -- off by default in CI's
# shared gate, on here since this script is meant to exercise everything
# real on a machine that has the toolchain; the test itself still skips
# gracefully if it doesn't.
OMNIFORGE_RUN_ANDROID_E2E=1 cargo test --workspace

echo "==> omni doctor"
cargo run -q -p omni-cli --manifest-path Cargo.toml -- doctor || true

echo "==> building examples/hello-web"
(
  cd examples/hello-web
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build web
  test -f dist/web/index.html
  test -f dist/web/robots.txt
  test -f dist/web/sitemap.xml
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- seo check web
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

echo "==> building examples/hello-node (requires a Node.js toolchain)"
(
  cd examples/hello-node
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build backend
  test -f dist/backend/output.txt
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/hello-typescript (requires Node.js + npm; resolves tsc on demand)"
(
  cd examples/hello-typescript
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build backend
  test -f dist/backend/output.txt
  test -f dist/backend/app/service.js
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/hello-go (requires a Go toolchain)"
(
  cd examples/hello-go
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build backend
  test -f dist/backend/output.txt
  test -f dist/build-report.json
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
)

echo "==> building examples/hello-windows (windows-desktop adapter, compiles + runs a real native .exe)"
(
  cd examples/hello-windows
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
  cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build windows
  test -f dist/windows/source.rs
  test -f dist/windows/output.txt
  test -f dist/build-report.json
  grep -q "Hello from hello-windows" dist/windows/output.txt
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

echo "==> building examples/hello-android (requires a real local Android SDK/JDK/Gradle; skipped otherwise)"
if cargo run -q -p omni-cli --manifest-path Cargo.toml -- doctor 2>&1 | grep -q "android-gradle .* — OK"; then
  (
    cd examples/hello-android
    cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- check
    cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- build android
    test -f dist/android/app-debug.apk
    test -f dist/build-report.json
    cargo run -q -p omni-cli --manifest-path ../../Cargo.toml -- clean
  )
else
  echo "    skipped: 'omni doctor' reports the android-gradle toolchain is not available on this machine"
fi

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
