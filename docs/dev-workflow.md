# Developer workflow: `omni dev`

`omni dev` builds and serves a project's enabled targets for local
development, managing any real backend dev server as a real, supervised
process — not just a build-then-serve-static-files command. See
[ADR 0009](decisions/0009-unified-dev-process-lifecycle.md) for why it's
built this way, and [adapter-protocol.md](adapter-protocol.md#dev-serve-capability-adapteryamls-optional-dev-block)
for the mechanism an out-of-process adapter opts into.

## Basic usage

```sh
omni dev                     # every enabled target that supports dev serving
omni dev web                 # just the web target
omni dev web backend         # both, explicitly
omni dev --port web=5173     # override one target's port
omni dev backend --port backend=9000
```

Each target that starts prints a line as soon as it's actually ready:

```
READY web http://127.0.0.1:4280
READY backend http://127.0.0.1:4281
```

A target that fails to build, has no available adapter, or whose adapter
doesn't support dev serving is reported and **skipped**, not treated as a
fatal error for the whole session — `omni dev` is a best-effort "show me
what's working" command, the same philosophy as `omni build --allow-partial`.
Only when *nothing* could be started does it exit non-zero.

Press Ctrl-C (or Ctrl-Break on Windows) to stop. `omni dev` then stops
every managed target — a real backend dev server gets a graceful stop
attempt (`SIGTERM` on Unix, `CTRL_BREAK_EVENT` on Windows) before a hard
kill if it doesn't respond within its configured grace period — and prints
what happened to each one before exiting.

## Configuring ports

Three ways, in priority order:

1. `--port <target>=<port>` on the command line (highest priority).
2. `targets.<name>.dev.port` in `omni.yaml`:
   ```yaml
   targets:
     backend:
       enabled: true
       adapter: python-backend
       dev:
         port: 4281
   ```
3. **Default: an auto-picked free ephemeral port.** If you don't configure
   anything, `omni dev` binds whatever the OS hands it and tells you in the
   `READY` line — no two targets can ever collide by default, because
   nothing is hardcoded.

If two targets end up pinned (via either of the first two mechanisms) to
the *same* port, `omni dev` reports `E-DEV-PORT-CONFLICT` and starts
nothing, rather than letting the second one fail to bind after the first
already started. If a pinned port is already held by something else
entirely, that target's startup fails with the real OS error — not a
silent no-op.

`examples/hello-fullstack` pins both targets (`web`: 4280, `backend`:
4281) specifically because its web target's rendered page has the
backend's URL baked in at build time (`web/app.yaml`'s `api_base` — see
[api-contract-format.md](api-contract-format.md) and
`docs/decisions/0003-template-only-rendering.md` for why the renderer
can't discover this at runtime instead). `examples/hello-web` and
`examples/hello-backend` leave ports unconfigured to demonstrate the
auto-pick default.

## What's actually running

- **In-process adapters** (`web-static`): `omni dev` serves `dist/<target>`
  as static files on a background thread it owns directly — GET-only, no
  TLS, one thread per connection, a 10s per-connection read timeout so a
  stalled client can't hang the server. Not a production web server; see
  "Development vs. production" below.
- **Out-of-process adapters that declare `dev`** (`python-backend`): a
  real child process, spawned the same safe way every adapter invocation
  is (no shell, controlled environment, `docs/security-model.md`), running
  the target's actual *built output* (not the adapter's own script) in its
  documented long-running mode.
- **Everything else**: reported as "built, but its adapter does not
  support live dev serving" — never faked.

While running, `omni dev` polls every managed child so an unexpected exit
(a real crash) is reported immediately rather than silently hung on; other
targets keep running.

## Development vs. production

`omni dev` is explicitly a local development convenience, not a deployment
mechanism:

- `omni build`/`omni check` never start or leave running any process —
  every adapter invocation they make is bounded and run-to-completion.
  Only `omni dev` starts long-lived processes, and only for the duration
  you run it.
- The static file server has no TLS, no caching headers beyond what a
  browser does by default, and no concurrency tuning — it's sized for "one
  developer's browser," not production traffic.
- A `python-backend` dev server is the adapter's packaged output running
  with Python's stdlib `http.server`/`ThreadingHTTPServer` (whatever the
  component script itself uses) — not a production WSGI/ASGI deployment.
  Running the packaged output in production is the operator's own
  decision and setup, outside OmniForge's scope.

## Troubleshooting

See [troubleshooting.md](troubleshooting.md) for the full table of stable
diagnostic codes, including `E-DEV-PORT-CONFLICT`,
`E-DEV-PORT-UNAVAILABLE`, `E-DEV-UNKNOWN-TARGET`, and the
`E-DEVSERVER-*`/`E-ADAPTER-TOOLCHAIN-MISSING` codes a dev-serve-capable
target's startup can report.
