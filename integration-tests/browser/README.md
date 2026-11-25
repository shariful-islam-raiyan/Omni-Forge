# Real-browser end-to-end tests (Playwright)

A small, real-browser-driven test suite for `examples/hello-fullstack`,
added in the "Unified Application Runtime and Developer Experience"
milestone. **This is additive to, not a replacement for, the Rust test
suite** (`cargo test --workspace`) — it covers something the Rust suite
structurally cannot: that a real browser's `fetch()` call, executed from
the real shipped `public/api-client.js`, against the real rendered
`dist/web/index.html`, actually reaches the real backend process and
updates the real DOM. `crates/omni-cli/tests/fullstack_http.rs` already
covers the HTTP contract itself at the protocol level without a browser;
this suite covers the part only a browser can: that the page's own
JavaScript, DOM, and fetch stack actually do the right thing.

## Why this lives outside the Cargo workspace

Playwright requires Node.js. Node cannot be a dependency of a Cargo test
target, so this is a separate, clearly-labeled directory with its own
`package.json`, not colocated under `crates/*/tests/` the way every other
integration test in this repository is (see `integration-tests/README.md`).
`cargo test --workspace`, `cargo build --workspace`, and
`scripts/verify.sh` do **not** run or require this suite — it is an
explicit, opt-in addition.

## Requirements

- Node.js 18 or newer (`node --version`).
- A built `omni` CLI binary: `cargo build -p omni-cli` (debug) or
  `cargo build -p omni-cli --release` from the repo root. `playwright.config.ts`
  looks for `target/release/omni[.exe]` first, then `target/debug/omni[.exe]`,
  and fails with a clear error if neither exists — it does **not** fall
  back to `cargo run` (which would silently mask a build failure behind a
  generic startup timeout).
- A Python 3 toolchain reachable the same way `omni doctor` checks for one
  (`python3`, `python`, or the Windows `py` launcher) — the real backend
  process this suite talks to needs it, exactly like every other
  `python-backend`-target build in this repository.

## Running it

```sh
cd integration-tests/browser
npm install
npx playwright install chromium
npm test
```

`npm test` starts the real `omni dev` process against
`examples/hello-fullstack` (Playwright's `webServer` config — see
`playwright.config.ts`), waits for the real web target to respond, runs
the tests in a real headless Chromium, and then tears the process down.

That teardown is a **hard stop** of the whole process tree, not the
graceful Ctrl-C/Ctrl-Break path `omni dev` otherwise supports — Node has no
reliable, cross-platform way to deliver the specific OS signal our own
`ctrlc`-based handler listens for. The graceful-shutdown guarantee itself
(real SIGTERM/CTRL_BREAK_EVENT delivery, a configurable grace period, then
a hard-kill fallback, and real child-process cleanup) is proven separately,
in Rust, where the exact signal delivered can actually be controlled and
asserted on: see `crates/omni-cli/tests/dev_lifecycle.rs` and
`crates/omni-adapter-api/src/devserver.rs`'s unit tests.

## What's covered

- The real page loads and its `<h1>` renders the configured title.
- The real `api-client.js`'s `fetch()` to the real backend's
  `GET /api/greeting` resolves and the DOM is updated with the real
  response (not a fixed/mocked string).
- A real browser-executed `fetch()` to `POST /api/echo` (from inside the
  page's own JS context) round-trips correctly for a valid request.
- The same endpoint returns the contract's declared 400 shape for an
  invalid request, verified from inside the browser.

## What's not covered (and why)

- There is no dedicated "echo form" UI element in this example today (see
  `examples/hello-fullstack/lib/components/ui.home/home.html.tmpl`) — the
  echo-endpoint tests above call `fetch` directly from the page's JS
  context instead of simulating a form submission that doesn't exist.
  Adding a real form is a product change to the example, not a test
  change; not done here to avoid changing the example beyond what this
  milestone's scope covers.
- Graceful shutdown (see above) — covered in Rust, not here.
- Firefox/WebKit — only Chromium is installed/run by the commands above.
  Playwright supports the others; nothing here prevents adding them later,
  they just aren't part of this milestone's verification.
