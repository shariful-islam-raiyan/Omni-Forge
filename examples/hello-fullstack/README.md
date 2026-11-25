# hello-fullstack

One OmniForge project, two enabled targets, one API contract. This example
exists to prove — not just claim — that a `web-static` frontend and a
`python-backend` service can be built from the same `omni.yaml` and actually
talk to each other over real HTTP.

## What's real here

- `svc.api` (`lib/components/svc.api/service.py`) is a real Python script.
  Run the normal way (`python service.py run-config.json`, exactly what
  `omni build backend` does), it behaves like every other python-backend
  component in this repo: prints a greeting, exits. Run with
  `--serve <port>`, it starts a real stdlib `http.server` implementing
  `GET /api/greeting` and `POST /api/echo`, exactly as declared in
  `backend/api-contract.json`.
- `ui.home` (`lib/components/ui.home/`) renders through the unmodified
  `web-static` adapter. Its template substitutes `{{api_base}}` into a
  `data-api-base` attribute using the adapter's existing `{{field}}`
  mechanism — no renderer change was needed.
- `public/api-client.js` is a small, real `fetch()`-based client, declared in
  `web/app.yaml`'s existing `public.js` list and copied verbatim into
  `dist/web/assets/` by the unmodified web-static adapter. It is genuine
  browser-side JavaScript — OmniForge does not execute it; a real browser does.

## Building both targets

```sh
cd examples/hello-fullstack
omni build all
```

This produces `dist/web/` (a static site) and `dist/backend/` (a packaged,
runnable Python application) independently, plus one `dist/build-report.json`
covering both targets. `omni build backend` on its own runs `svc.api/service.py`
once, in its default one-shot mode, to produce `dist/backend/output.txt` as
proof of execution — it does **not** start the HTTP server (a build must
finish in finite time and must not leave a server running as a side effect;
see `docs/decisions/0007-backend-dev-server-is-example-scoped.md`).

## Running the full stack with `omni dev`

```sh
cd examples/hello-fullstack
omni dev
```

This is now a real, unified dev workflow (see
[docs/dev-workflow.md](../../docs/dev-workflow.md) and
[ADR 0009](../../docs/decisions/0009-unified-dev-process-lifecycle.md),
which supersedes ADR 0007's original "manual step" framing below):
`omni dev` builds both targets, starts the backend's `--serve <port>` mode
itself as a real, supervised child process (bound to the port configured
in this project's `omni.yaml`, `4281`, matching `web/app.yaml`'s baked-in
`api_base`), and serves the web target's static output on `4280`. Open
`http://127.0.0.1:4280` in a real browser — the page's real `fetch()` call
reaches the real backend. Press Ctrl-C to stop both cleanly.

You can still run the backend by hand exactly as before (useful for
debugging the script in isolation, outside `omni dev`'s supervision):

```sh
cd dist/backend
python app/service.py run-config.json --serve 8765
```

## What's automated vs. manually verified

- `crates/omni-cli/tests/fullstack_http.rs` starts this exact backend server
  as a real subprocess and makes real HTTP requests against it (success,
  invalid request data, an unknown route, a startup failure, a timeout, a
  deliberate contract violation, an invalid contract failing the build, and
  a contract edit busting the build cache), validating responses against
  `backend/api-contract.json`. This is the **HTTP-contract level** of
  testing.
- `crates/omni-cli/tests/dev_lifecycle.rs` drives the unified `omni dev`
  runtime itself against this example: both targets starting and
  responding, graceful shutdown really stopping the real backend process
  and freeing its port, port conflicts, a missing toolchain, and an
  unexpected mid-session crash. This is the **process-lifecycle level**.
- `integration-tests/browser/` drives a real headless Chromium against a
  real `omni dev` process for this example: the page loads, the real
  `fetch()` call in `public/api-client.js` renders a real backend response,
  and a real browser-executed `fetch` round-trips `POST /api/echo`. This is
  the **real-browser level** — see that directory's README for setup (it
  needs Node.js and is not part of `cargo test`). Building and running
  this suite for real is exactly how a genuine bug (the backend never sent
  `Access-Control-Allow-Origin`, so a real browser's cross-origin `fetch`
  silently failed) was found and fixed — "manually verified" had not
  actually caught it.
