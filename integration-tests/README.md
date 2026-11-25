# Integration tests

End-to-end tests live next to the code they exercise, where `cargo test` can
find and run them without any extra wiring. **One documented exception:**
`integration-tests/browser/` holds the real-browser (Playwright) suite —
it needs Node.js, which cannot live inside a Cargo test target, so it is a
small, separate, clearly-labeled addition rather than being colocated with
the Rust tests. See [browser/README.md](browser/README.md).

- `crates/omni-cli/tests/cli.rs` — drives the real `omni` binary (via
  `assert_cmd`) through `create` → `check` → `build` → `clean`, plus failure
  paths (invalid schema, missing component, unknown target, non-empty
  create target, etc.).
- `crates/omni-cli/tests/build_all_partial_failure.rs` — proves `omni build
  all` reports each enabled target's status independently: one target
  failing doesn't erase another target's already-written artifacts, and
  the overall exit code reflects the failure.
- `crates/omni-cli/tests/fullstack_http.rs` (+ shared helpers in
  `crates/omni-cli/tests/support/mod.rs`) — builds `examples/hello-fullstack`'s
  real `backend` target, starts the packaged Python script as a real HTTP
  server, and makes real HTTP requests against it, validated against
  `examples/hello-fullstack/backend/api-contract.json`. Covers a successful
  request, invalid request data, an unknown route, a backend startup
  failure, a startup timeout, an HTTP-level read timeout, and a deliberate
  contract violation. See `examples/hello-fullstack/README.md` for scope
  (no browser is driven by this suite).
- `crates/omni-web-adapter/tests/compile.rs` — exercises the web-static
  adapter's `compile`/`validate` against a synthetic project tree,
  independent of the CLI.
- `crates/omni-core/tests/fixtures.rs` — runs the loader against the static
  fixtures in `fixtures/valid-projects` and `fixtures/invalid-projects`.

This directory exists to document that mapping, rather than to hold a second
copy of the same tests. Run the Rust suite with:

```
cargo test --workspace
```

Run the browser suite (requires Node.js 18+; see
[browser/README.md](browser/README.md) for exact setup) with:

```
cd integration-tests/browser
npm install
npx playwright install chromium
npm test
```
