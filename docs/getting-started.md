# Getting started

## 1. Build the CLI

```sh
git clone <this repo>
cd omniforge
cargo build --workspace
```

Requires a stable Rust toolchain (see `rust-toolchain.toml`) and network
access to crates.io for the first build. The `web` target needs no other
toolchain — `web-static` renders in-process. The `backend` target needs a
Python 3 interpreter on `PATH` under one of `python3`, `python`, or (on
Windows) the `py` launcher — run `omni doctor` to check.

## 2. Try the shipped example

```sh
cd examples/hello-web
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- check
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- build web
```

You should see:

```
target 'web': OK
==> building target 'web'
    target 'web': SUCCESS (3 artifact(s))

build report written to .../dist/build-report.json
build SUCCESS
```

Open `dist/web/index.html` in a browser directly, or serve it:

```sh
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- dev web
```

`hello-web` doesn't pin a dev port in `omni.yaml`, so you'll see a
`READY web http://127.0.0.1:<port>` line with an auto-picked free port —
open that URL. You'll see a real page: an `<h1>` with the title from
`web/app.yaml`, a message from the component's default, a footer, and the
stylesheet from `public/style.css` applied. Press Ctrl-C to stop it; see
[dev-workflow.md](dev-workflow.md) for running multiple targets at once,
pinning ports, and what actually gets stopped on exit.

Clean up generated output when you're done:

```sh
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- clean
```

## 3. Install `omni` and start a new project

```sh
cargo install --path crates/omni-cli
omni create my-site
cd my-site
omni check
omni build web
cat dist/web/index.html
```

`omni create` scaffolds from `templates/basic`: `omni.yaml`, one `ui.home`
component with a `.html.tmpl` template, `web/app.yaml`, and
`public/style.css`.

## 4. Make a change and see the cache work

```sh
# Edit web/app.yaml: change root.inputs.title
omni build web   # rebuilds, cache miss, new content in dist/web/index.html
omni build web   # unchanged inputs -> prints "(cache hit)"
```

## 5. What to edit

- **`omni.yaml`** — targets, build output location, caching.
- **`web/app.yaml`** — page title, which component is the root, what
  values it receives, which CSS/JS/assets from `public/` to include.
- **`lib/components/ui.home/component.yaml`** — declares `title`
  (required) and `message` (optional, with a default) as the component's
  inputs.
- **`lib/components/ui.home/home.html.tmpl`** — the only place that
  actually contains markup. `{{title}}` and `{{message}}` are the only
  kind of placeholder supported (see
  [docs/component-specification.md](component-specification.md) and
  [decisions/0003-template-only-rendering.md](decisions/0003-template-only-rendering.md)
  for why there's nothing fancier).

## 6. Try the backend example (out-of-process adapter)

```sh
cd examples/hello-backend
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- check
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- build backend
cat dist/backend/output.txt
```

`check` and `build` each spawn a real Python process
(`adapters/python-backend/adapter.py`) that validates and then actually
runs `lib/components/svc.greeter/service.py` — `dist/backend/output.txt`
is that script's real captured stdout, not a fabricated success message.
`dist/backend/app/service.py` plus `run.sh`/`run.bat` form a runnable
package independent of OmniForge:

```sh
cd dist/backend
python3 app/service.py run-config.json   # or: ./run.sh
```

See [component-specification.md](component-specification.md#service-components-and-the-python-backend-adapter)
for the component contract and [adapter-protocol.md](adapter-protocol.md)
for what actually gets sent to the adapter process.

## 7. Try the full-stack example (`omni dev`, both targets at once)

```sh
cd examples/hello-fullstack
cargo run --manifest-path ../../Cargo.toml -p omni-cli -- dev
```

You'll see each target build, then a `READY <target> http://127.0.0.1:<port>`
line as it starts serving — `backend` first (a real, supervised Python
HTTP server process), then `web` (the static page, whose `api_base` is
baked in at build time to match `backend`'s configured port — see
`examples/hello-fullstack/web/app.yaml`). Open the `web` URL in a browser:
the page's own JavaScript (`public/api-client.js`) fetches from the real
backend and shows its response. Press Ctrl-C: `omni dev` stops the
backend process (graceful signal first, hard kill only if that doesn't
work within its grace period) and the static server, and prints what
happened to each before exiting. See [dev-workflow.md](dev-workflow.md)
for port configuration and what happens when a target fails to start.

## 8. When something fails

Every failure prints a stable code (`E-...`) plus, usually, a suggestion.
See [troubleshooting.md](troubleshooting.md) for the ones you're most
likely to hit while learning the format, or run `omni check` first — it
catches configuration and resolution problems without needing a full
build.

## Current scope reminder

This walkthrough covers `web` and `backend`, the two targets with a real
adapter today. `android`, `ios`, `windows`, `linux`, and `macos` are valid
target *names* (so `omni.yaml` won't reject them for existing), but
enabling one without a matching adapter will correctly fail with
`E-BUILD-ADAPTER-UNAVAILABLE` — see [docs/roadmap.md](roadmap.md) for
what's planned there.
