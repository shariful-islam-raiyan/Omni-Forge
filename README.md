# OmniForge

[![CI](https://github.com/shariful-islam-raiyan/Omni-Forge/actions/workflows/ci.yml/badge.svg)](https://github.com/shariful-islam-raiyan/Omni-Forge/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](rust-toolchain.toml)

**A language-neutral build-orchestration framework: one project
specification, one component/contract system, one CLI — multiple build
targets, each produced by a dedicated adapter.**

OmniForge is not a Rust framework, a JavaScript framework, or a Python
framework. Rust currently implements the *core engine and CLI* — the part
that loads your project, validates it, resolves dependencies, and
orchestrates a build — while *adapters* do the actual language-specific
work (rendering a web template, running a Python script, and so on).
Nothing about the project format, the component model, or the adapter
protocol is Rust-specific; a future adapter can be written in any
language that can be spawned as a process and speak the documented wire
protocol.

---

## Table of contents

- [What OmniForge is](#what-omniforge-is)
- [Vision and design philosophy](#vision-and-design-philosophy)
- [Why OmniForge exists](#why-omniforge-exists)
- [How this differs from using each tool directly](#how-this-differs-from-using-each-tool-directly)
- [Architecture overview](#architecture-overview)
- [Current implementation status](#current-implementation-status)
- [Supported targets and languages](#supported-targets-and-languages)
- [Explicitly unsupported / planned](#explicitly-unsupported--planned)
- [Project structure](#project-structure)
- [Installation](#installation)
- [Requirements and prerequisites](#requirements-and-prerequisites)
- [Quick start](#quick-start)
- [CLI command reference](#cli-command-reference)
- [Example walkthrough: a static site](#example-walkthrough-a-static-site)
- [Node.js backend walkthrough](#nodejs-backend-walkthrough)
- [Full-stack walkthrough: web + Python backend](#full-stack-walkthrough-web--python-backend)
- [Adapter architecture and extension guide](#adapter-architecture-and-extension-guide)
- [Components and the API contract format](#components-and-the-api-contract-format)
- [Building, developing, testing, and cleaning](#building-developing-testing-and-cleaning)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [Roadmap](#roadmap)
- [License](#license)
- [Acknowledgements](#acknowledgements)

---

## What OmniForge is

A project describes itself once, in `omni.yaml`: a set of **components**
(small units of UI or service logic) and a set of **targets** (`web`,
`backend`, and names reserved for future platforms). Each enabled target
names an **adapter** — the thing that actually knows how to turn a
component into real output for that target. The core engine
(`omni-core`) never parses JavaScript, Python, or any other language; it
only understands OmniForge's own versioned specification documents
(`omni.yaml`, `component.yaml`, `adapter.yaml`, …) and delegates every
piece of real work to an adapter through a documented interface
(`omni-adapter-api`).

Today, two real adapters exist:

- **`web-static`** — runs in-process inside the `omni` binary. Renders a
  small, deliberately non-Turing-complete `{{field}}` template into a
  real static website (HTML/CSS/JS), with every input HTML-escaped.
- **`python-backend`** — runs out-of-process. Spawns a real Python 3
  interpreter, speaking a versioned JSON protocol over stdin/stdout, to
  validate and execute a real backend script and package it into a
  runnable artifact.

The `omni` CLI ties this together: it validates your project, resolves
which component implements which target, builds (with a real
content-addressed local cache), and — new in this release — runs a
**unified development server** that builds and serves every enabled
target at once, managing a real backend process with graceful shutdown.

## Vision and design philosophy

The long-term goal is a single, language-neutral project system that can
orchestrate building, testing, running, and packaging applications across
multiple languages and platforms through adapters — without ever asking
you to rewrite your application logic in the orchestrator's own language.

Concretely, that means:

- **The core stays language-neutral, permanently.** `omni-core` and
  `omni-adapter-api` must never need to understand JavaScript, Python,
  Go, or any other target language's syntax. If a feature requires that,
  it belongs in an adapter, not the core — this is an enforced project
  rule, not a suggestion (see [CONTRIBUTING.md](CONTRIBUTING.md)).
- **Everything has a versioned schema and a stable contract.** Every
  document OmniForge reads or writes (`omni.yaml`, `component.yaml`,
  `adapter.yaml`, build reports, the adapter wire protocol) is versioned
  JSON Schema, validated before use, with an explicit compatibility
  policy for the next version.
- **Honest status over impressive claims.** A feature is "Implemented"
  only once it has a passing test exercising the real thing (a real
  subprocess, a real HTTP request, a real browser) — never because a type
  or a manifest entry exists. See
  [Current implementation status](#current-implementation-status).
- **No sandboxing claims without a sandbox.** Where a real security
  boundary doesn't exist yet, the documentation says so explicitly rather
  than implying one. See [docs/security-model.md](docs/security-model.md).

## Why OmniForge exists

Modern applications routinely span more than one language and runtime —
a web frontend, a backend service, sometimes a mobile or desktop client —
each with its own build tool, its own configuration format, its own way
of expressing "build this," "run this for development," and "here's what
went wrong." Keeping these in sync (ports, contracts, build order,
dev-server lifecycles) is usually solved per-project, by hand, with
scripts that don't generalize.

OmniForge's bet is that the *coordination* layer — project
structure, component boundaries, dependency resolution, build caching,
dev-server lifecycle management, and a declared contract between pieces
written in different languages — is largely language-independent, and
can be built once, correctly, with real tests, instead of being
re-invented per project or per language ecosystem.

## How this differs from using each tool directly

Nothing above replaces your language's own compiler, bundler, or test
runner — an adapter *calls* those. What OmniForge adds on top:

| Without OmniForge | With OmniForge |
|---|---|
| Each language's tool has its own config format, invoked separately. | One project file (`omni.yaml`) describes every target; one CLI (`omni`) drives all of them. |
| Starting a frontend dev server and a backend dev server is two terminals, two manual steps, and you track ports yourself. | `omni dev` builds and starts every enabled target at once, auto-picks free ports (or lets you pin them), prints each one's URL, and stops everything cleanly on Ctrl-C — including a real graceful-then-hard-kill shutdown of the backend process. |
| "Does the frontend's expected API shape match what the backend actually returns?" is usually checked by hand or not at all. | A declared contract (`api-contract.json`) is schema-validated as part of the build and checked against real HTTP responses in tests — see [Components and the API contract format](#components-and-the-api-contract-format). |
| A stale build cache serving wrong output is a per-tool, per-project risk you manage yourself. | One content-addressed cache, with a documented, tested key (component source, entry file, declared contract, `public/` assets) — editing any of those busts the cache. |
| Partial build failures (one target broken, others fine) often abort everything or require custom scripting. | `omni build all`/`omni dev` report each target's status independently; one target failing doesn't erase another's output. |

What OmniForge does **not** do: replace your compiler/bundler, run
arbitrary code in any language you point it at, or provide a sandboxed
execution environment. See [Limitations](#explicitly-unsupported--planned)
and [docs/security-model.md](docs/security-model.md).

## Architecture overview

```
omni.yaml ──► omni-core (load, validate, discover, resolve, plan, cache)
                   │
                   ▼
    omni-adapter-api (Adapter trait + out-of-process execution engine
                       + long-lived dev-server process manager)
                   │
          ┌────────┴─────────┐
          ▼                  ▼
  omni-web-adapter     python-backend adapter
  (in-process Rust)    (spawned Python process, JSON over stdio)
          │                  │
          ▼                  ▼
   dist/web/*          dist/backend/* (incl. a runnable package)
          └────────┬─────────┘
                    ▼
            dist/build-report.json

omni dev: builds every enabled target the same way, then either serves
it (web-like targets, in-process static server) or starts it as a real,
supervised child process (an adapter-declared "dev" capability) — both
stopped cleanly on Ctrl-C.
```

Full details, including a data-flow diagram and the rationale behind
specific design trade-offs, are in [docs/architecture.md](docs/architecture.md).
Key design decisions are recorded as ADRs in
[docs/decisions/](docs/decisions/).

## Current implementation status

Status definitions (used consistently across this README and
`docs/roadmap.md`): **Implemented** — has passing tests and works today.
**Partial** — the type/field/hook exists and is exercised, but the
behavior it implies isn't fully realized. **Planned** — designed or
discussed, nothing built. **Blocked** — needs a decision or external
resource this project doesn't have yet.

| Area | Status | Notes |
|---|---|---|
| Core engine (load/validate/discover/resolve/cache/diagnostics) | Implemented | `crates/omni-core`, 44 unit tests |
| Adapter interface + out-of-process execution engine | Implemented | `crates/omni-adapter-api` |
| `web-static` adapter | Implemented | In-process, escaped-template rendering only |
| `python-backend` adapter | Implemented | Real Python subprocess, stdlib-only |
| `node-backend` adapter | Implemented (Windows verified; macOS/Linux unverified) | Real Node.js subprocess, zero npm dependencies; same `backend` target as `python-backend` — see [ADR 0012](docs/decisions/0012-node-backend-adapter.md) and `examples/hello-node` |
| `typescript-backend` adapter | Implemented (Windows verified; macOS/Linux unverified) | Real `tsc` type-checking (not just transpiling) + compile + real Node execution, same `backend` target — see [ADR 0013](docs/decisions/0013-typescript-backend-adapter.md) and `examples/hello-typescript` |
| `go-backend` adapter | Implemented (Windows verified; macOS/Linux unverified) | Real `go build` to a genuine native executable, run directly (no interpreter) — see [ADR 0015](docs/decisions/0015-go-backend-adapter.md) and `examples/hello-go` |
| `android-gradle` adapter | Implemented (Windows verified; macOS/Linux unverified) | Real Gradle invocation produces a real, verified debug APK; see [ADR 0011](docs/decisions/0011-android-gradle-adapter.md) and `examples/hello-android` |
| Unified `omni dev` (multi-target, real process lifecycle) | Implemented | Graceful-then-hard-kill shutdown, verified against the real compiled binary with a genuine OS signal |
| API contract format + validation | Implemented | A **custom** format (not OpenAPI — see below), with build-time validation and reusable request/response validators |
| Full-stack example (web ↔ backend over a declared contract) | Implemented | `examples/hello-fullstack`, checked at the HTTP, process-lifecycle, and real-browser levels |
| Local build cache | Implemented | Content-addressed, scoped (see [docs/caching.md](docs/caching.md)) |
| Component composition (one component's rendered output embedded in another's) | Implemented | `composes` in `component.yaml`; `web-static` only; see [ADR 0010](docs/decisions/0010-component-composition-via-slots.md) and `examples/navbar-demo`. A slot's inputs are static, not interpolated from the parent's own inputs — see that ADR's Consequences |
| SEO foundations (metadata, canonical, Open Graph, `robots.txt`, `sitemap.xml`, minimal JSON-LD, `omni seo check`/`report`) | Implemented | Honestly scoped to `web-static`'s real one-page output — see [ADR 0014](docs/decisions/0014-seo-foundations.md) and [docs/seo.md](docs/seo.md) |
| `omni test` | Partial | Reports honestly that no adapter implements the `test` operation yet — never fakes a pass |
| Parallel build scheduling across targets | Implemented | `omni.yaml`'s `build.parallel: true` builds independent targets concurrently (one OS thread each); default stays `false` (sequential, unchanged). Does not parallelize within one target's own component graph yet |
| Third-party adapter trust/enablement | Planned | Only adapters this repository ships and reviews can run today |
| Adapter sandboxing beyond argv/env/cwd/timeout | Blocked | No third-party adapter exists yet to sandbox; see security model |
| Desktop targets (Windows/Linux/macOS) | Planned | Vocabulary exists (`omni.yaml` accepts the target names); no adapter |
| Android target | Implemented (Windows verified; macOS/Linux unverified) | `android-gradle` adapter — real Gradle build, real APK, see [ADR 0011](docs/decisions/0011-android-gradle-adapter.md) |
| iOS target | Planned | Vocabulary only, no adapter — genuinely requires a macOS/Xcode environment this project's development machine (Windows) does not have |
| Additional language adapters (Go/Rust/C/C++/Java/Kotlin/Dart/Swift/COBOL) | Planned | `ProcessAdapter` is language-neutral and reusable — proven by `node-backend` and `typescript-backend` (above); none of these others exist yet |
| Real OpenAPI 3.1 document support | Planned | The current contract format is deliberately smaller and custom instead — see [ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md) |

The full, continuously-updated, item-by-item table (every row backed by a
file and a test) is [docs/roadmap.md](docs/roadmap.md) — treat this
README's table as a summary, that one as the source of truth.

## Supported targets and languages

| Target name | Adapter | Status | What it actually produces |
|---|---|---|---|
| `web` | `web-static` | **Implemented** | A real static site: HTML rendered from escaped templates, copied CSS/JS/assets, a build manifest |
| `backend` | `python-backend` | **Implemented** | A real, packaged, runnable Python application (`app/`, `run.sh`/`run.bat`, captured output) — requires a Python 3 interpreter on `PATH` |
| `backend` | `node-backend` | **Implemented** (Windows verified; macOS/Linux unverified) | A real, packaged, runnable Node.js application, same artifact layout as `python-backend` — requires a Node.js interpreter on `PATH`; see [ADR 0012](docs/decisions/0012-node-backend-adapter.md) |
| `backend` | `typescript-backend` | **Implemented** (Windows verified; macOS/Linux unverified) | Real `tsc` type-checking + compile + Node execution — requires Node.js + npm (TypeScript itself resolved on demand); see [ADR 0013](docs/decisions/0013-typescript-backend-adapter.md) |
| `backend` | `go-backend` | **Implemented** (Windows verified; macOS/Linux unverified) | A real, standalone native executable, compiled by a real `go build` and run directly — requires a Go toolchain on `PATH`; see [ADR 0015](docs/decisions/0015-go-backend-adapter.md) |
| `android` | `android-gradle` | **Implemented** (Windows verified; macOS/Linux unverified) | A real, debug-signed APK — requires a real local Android SDK, JDK, and a discoverable Gradle distribution (`omni doctor` reports exactly what was found); see [ADR 0011](docs/decisions/0011-android-gradle-adapter.md) |
| `ios`, `windows`, `linux`, `macos` | — | **Not implemented** | These are valid target *names* (so `omni.yaml` won't reject them for existing), but enabling one with no adapter correctly fails with `E-BUILD-ADAPTER-UNAVAILABLE` rather than pretending to succeed |

OmniForge's vocabulary is intentionally broader than its current adapter
set — see [Explicitly unsupported / planned](#explicitly-unsupported--planned).

## Explicitly unsupported / planned

To be unambiguous about what **not** to expect from the current release:

- **No iOS, Windows desktop, Linux desktop, or macOS desktop adapter
  exists.** Enabling one of these targets produces an honest
  `unsupported` build result, never a fake success. iOS genuinely
  requires a macOS/Xcode environment — not available on this project's
  (Windows) development machine — so it was not attempted; see
  [docs/roadmap.md](docs/roadmap.md).
- **Android support is real but narrow.** `android-gradle` builds a
  real, verified debug APK with exactly one `Activity`/one `TextView`
  from a component's declared message — no emulator/device install, no
  release signing, no `.aab` output, and not a UI framework. See
  [ADR 0011](docs/decisions/0011-android-gradle-adapter.md).
- **No Rust-as-a-backend, Dart, C, C++, Java, Swift, or COBOL adapter
  exists.** `ProcessAdapter` (the out-of-process execution mechanism) is
  language-neutral and designed to be reused for any of these — proven
  concretely by `node-backend`, `typescript-backend`, and `go-backend`
  (three independent out-of-process adapters requiring zero changes to
  `omni-adapter-api`'s public surface; see
  [ADR 0012](docs/decisions/0012-node-backend-adapter.md)/
  [ADR 0013](docs/decisions/0013-typescript-backend-adapter.md)/
  [ADR 0015](docs/decisions/0015-go-backend-adapter.md), the last of
  which — unlike the first two — compiles to a real native executable
  and runs it directly, no interpreter involved) — but none of these
  *other* languages has actually been built and tested yet.
  (Kotlin is used by the real `android-gradle` adapter above, but only to
  generate one fixed Activity template — not as a general-purpose adapter
  language.)
- **The web adapter is not a React/Vue/Angular/Flutter-style component
  framework.** It renders a deliberately tiny, non-Turing-complete
  `{{field}}` substitution template — no virtual DOM, no client-side
  router, no component lifecycle, no JSX/hooks equivalent. See
  [ADR 0003](docs/decisions/0003-template-only-rendering.md) for why.
- **The API contract format is not OpenAPI 3.1 compatible**, and does
  not claim to be. An earlier description of it that way was inaccurate
  and has been corrected — see
  [ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md) for
  exactly why (no `openapi` field, no `content`/media-type wrapping,
  JSON Schema draft-07 rather than OpenAPI 3.1's 2020-12-based dialect).
  It's a small, custom, OpenAPI-*inspired* format.
- **No third-party or dynamically-loaded adapters.** Both shipped
  adapters are reviewed code embedded in this repository and binary at
  compile time. Nothing is discovered on disk or downloaded.
- **No sandboxing beyond argv/environment/working-directory/timeout
  control.** An adapter's child process runs with the same privileges as
  the `omni` CLI itself — see [docs/security-model.md](docs/security-model.md).

See [docs/roadmap.md](docs/roadmap.md) for the full breakdown and
[ROADMAP.md](ROADMAP.md) for the high-level direction.

## Project structure

```
specs/                     Versioned JSON Schemas for every document type
                            OmniForge reads (omni.yaml, component.yaml,
                            adapter.yaml, the web app manifest, build
                            reports, the API contract format).
crates/omni-core/          Language-neutral spec types, validation,
                            component discovery, dependency graph,
                            resolver, build cache, path-safety primitives.
crates/omni-adapter-api/   The Adapter trait, the out-of-process
                            execution engine (process.rs), the long-lived
                            dev-server process manager (devserver.rs),
                            the wire protocol types, and adapter
                            conformance checks.
crates/omni-web-adapter/   The web-static adapter: template rendering,
                            asset copying, build-manifest generation.
crates/omni-cli/           The `omni` binary (and a lib target, for
                            white-box adapter tests): command wiring,
                            the build orchestrator, the unified dev-
                            process runtime (devrun.rs), the dev-mode
                            static file server, and the python-backend
                            adapter's Rust-side wiring, plus the
                            node-backend adapter's equivalent wiring.
adapters/web-static/       The web-static adapter's manifest.
adapters/python-backend/   The python-backend adapter's manifest AND its
                            actual implementation — a real, stdlib-only
                            Python script, embedded into the omni binary
                            at compile time.
adapters/node-backend/     The node-backend adapter's manifest AND its
                            actual implementation — a real, dependency-free
                            Node.js script, embedded into the omni binary
                            at compile time; see ADR 0012.
templates/basic/           What `omni create` scaffolds — also embedded
                            into the binary at compile time.
examples/hello-web/        A complete, buildable web example project.
examples/hello-backend/    A complete, buildable backend example project
                            (python-backend).
examples/hello-node/       A complete, buildable backend example project
                            (node-backend) — same shape as hello-backend,
                            different interpreter.
examples/hello-fullstack/  Web + backend together, one API contract, a
                            real browser-side API client — see its README.
examples/navbar-demo/      Component composition: a ui.page component
                            embeds a reusable ui.navbar component's
                            rendered output via 'composes' — see its
                            README and ADR 0010.
examples/hello-android/    A real, buildable Android app (one Activity,
                            one TextView) via the android-gradle adapter
                            — see its README and ADR 0011.
crates/omni-android-adapter/ The android-gradle adapter: toolchain
                            discovery (SDK/JDK/Gradle), Gradle project
                            generation, Kotlin-string templating, and a
                            real Gradle invocation + APK verification.
adapters/android-gradle/   The android-gradle adapter's manifest.
integration-tests/browser/ The real-browser (Playwright) E2E suite —
                            additive, Node-based, outside `cargo test`.
fixtures/                  Static valid/invalid omni.yaml fixtures used
                            by tests.
docs/                      Architecture, specs, protocol, security,
                            roadmap, and numbered Architecture Decision
                            Records (docs/decisions/).
```

## Installation

OmniForge ships as a single, self-contained binary (`omni`) — every
adapter manifest, the Python adapter's script, every JSON Schema, and
the project templates `omni create` scaffolds are **embedded into the
binary at compile time** (`include_str!`, not read from disk at
runtime). This was verified directly: building from a plain copy of this
repository placed in an unrelated directory, with no relationship to the
original checkout, and running the resulting binary from a third,
unrelated directory — `omni --version`, `doctor`, `adapter list`,
`create`, `check`, `build`, and `dev` all worked correctly with nothing
but the installed executable.

### Option 1: `cargo install` from GitHub (recommended for most users)

Requires a stable [Rust toolchain](https://rustup.rs) (Cargo included).
No separate clone, no navigating into crate directories:

```sh
cargo install --git https://github.com/shariful-islam-raiyan/Omni-Forge --branch main omni-cli
omni --version
omni doctor
omni create my-project
cd my-project
omni check
omni build web
omni dev
```

This clones the repository to a temporary build directory, compiles the
`omni-cli` package (and, with it, `omni-core`/`omni-adapter-api`/
`omni-web-adapter`), and installs the resulting `omni` binary to Cargo's
install root (`~/.cargo/bin` by default — make sure that's on your
`PATH`, exactly as `cargo install` always requires).

### Option 2: build from a local clone (for contributors, or if you want to inspect the source)

```sh
git clone https://github.com/shariful-islam-raiyan/Omni-Forge.git
cd Omni-Forge
cargo install --path crates/omni-cli
```

Functionally identical to Option 1 — this is exactly what was verified
locally before this was documented (see above); Option 1 just skips the
manual `git clone` step.

### Option 3: run without installing (for trying it out once)

```sh
git clone https://github.com/shariful-islam-raiyan/Omni-Forge.git
cd Omni-Forge
cargo run -p omni-cli -- --version
cargo run -p omni-cli -- create my-project
```

### Prebuilt binaries (GitHub Releases)

**Not published yet.** `cargo install --git`/`--path` (above) is the
verified, working installation path today. Prebuilt binaries via GitHub
Releases are a reasonable future addition (tracked in
[ROADMAP.md](ROADMAP.md)) once there's a tagged release to build them
from — this README will be updated with exact commands if/when that
exists, not before.

## Requirements and prerequisites

- **Rust, stable** (see [`rust-toolchain.toml`](rust-toolchain.toml)) and
  Cargo — always required, for every target.
- **For the `backend` target, one of**: a Python 3 interpreter reachable
  as `python3`, `python`, or (on Windows) the `py` launcher (for
  components using `python-backend`), or a Node.js interpreter reachable
  as `node` (or, on Debian/Ubuntu, `nodejs`) (for components using
  `node-backend`) — a project only needs the one its components actually
  declare. The `web` target needs nothing beyond Rust — `web-static`
  renders in-process. Run `omni doctor` after installing to check what's
  actually available on your machine.
- **For contributing, or the optional real-browser test suite only**:
  Node.js 18+ — see [integration-tests/browser/README.md](integration-tests/browser/README.md).
  Not needed to install or use the CLI itself.
- **For the `android` target only**: a real Android SDK (at least one
  platform + build-tools), a JDK, and a Gradle distribution discoverable
  on this machine (see [ADR 0011](docs/decisions/0011-android-gradle-adapter.md)
  for exactly how each is found — typically all three already exist if
  Android Studio is installed). Run `omni doctor` to check.

## Quick start

```sh
omni create hello-world
cd hello-world
omni check
omni build web
omni dev             # serves every enabled target; prints each one's URL; Ctrl-C to stop
```

Several complete, ready-made examples ship in this repository — see
[docs/getting-started.md](docs/getting-started.md) for a full guided
walkthrough of the first four:

- [`examples/hello-web`](examples/hello-web) — the in-process
  `web-static` adapter.
- [`examples/hello-backend`](examples/hello-backend) — the out-of-process
  `python-backend` adapter; `omni build backend` there really spawns
  Python and runs a real script.
- [`examples/hello-node`](examples/hello-node) — the out-of-process
  `node-backend` adapter; same shape as `hello-backend`, a real Node.js
  interpreter instead — see [Node.js backend walkthrough](#nodejs-backend-walkthrough)
  above.
- [`examples/hello-fullstack`](examples/hello-fullstack) — web + Python
  backend at once, talking to each other over a declared, validated
  contract; see [Full-stack walkthrough](#full-stack-walkthrough-web--python-backend)
  below.
- [`examples/navbar-demo`](examples/navbar-demo) — component
  composition: a `ui.page` component embeds a separately-authored,
  reusable `ui.navbar` component's rendered output via `composes` — see
  [ADR 0010](docs/decisions/0010-component-composition-via-slots.md).
- [`examples/hello-android`](examples/hello-android) — the `android-gradle`
  adapter; `omni build android` really invokes Gradle — see
  [Android walkthrough](#android-walkthrough-a-real-built-apk) below.

## CLI command reference

| Command | What it does |
|---|---|
| `omni --version` | Prints the installed CLI version. |
| `omni create <name>` | Scaffolds a new project from the embedded `templates/basic`. Refuses to touch a non-empty directory. (Web-target scaffolding only today.) |
| `omni check [--target <t>] [--json]` | Validates `omni.yaml`, discovers and validates components, and resolves each enabled (or specified) target without writing build output. For `backend`, this really spawns the Python adapter process in validate mode. |
| `omni adapter list [--json]` | Lists adapters compiled into this CLI, their protocol (`in-process/v1` or `out-of-process/v1`), declared capabilities, and toolchain requirements. |
| `omni adapter validate <path> [--json]` | Validates a third-party or in-progress adapter manifest on its own (schema + conformance) — accepts a directory or a direct `adapter.yaml` path. Does not register, load, or execute it. |
| `omni doctor` | Checks the local Rust toolchain and each adapter's declared external tool requirements — actually invokes them (trying every known alias, e.g. `python3`/`python`/`py`), never assumes. |
| `omni build <target\|all> [--allow-partial]` | Builds one target or every enabled target. Writes `dist/<target>/...` and `dist/build-report.json`. Fails (exit 1) if any enabled target has no available adapter, unless `--allow-partial` is passed. |
| `omni dev [targets...] [--port NAME=PORT]` | Builds and serves every enabled target (or just the ones named) at once. `web`-like targets get a built-in static HTTP server; a target whose adapter declares dev-serve support (today: `backend`) gets a real, supervised, long-lived child process with graceful-then-hard-kill shutdown on Ctrl-C. Ports auto-pick free unless pinned via `omni.yaml` or `--port`. See [docs/dev-workflow.md](docs/dev-workflow.md). |
| `omni test` | Validates the project and asks each enabled target's adapter to run its `test` operation. No shipped adapter implements `test` yet — reported honestly, never faked. |
| `omni package <target\|all>` | Archives an already-built target's `dist/<target>/` into `dist/<target>.zip`. Run `omni build <target>` first — packaging and building are separate, explicit steps. Generic across every target; not adapter-specific. |
| `omni clean` | Removes the project's build output directory, after verifying it resolves strictly inside the project root. |
| `omni seo check <target> [--json]` | Real technical-SEO checks against an already-built target's `index.html` (title, description, viewport, canonical, single `<h1>`, image `alt` text, indexability). Exits non-zero on any failure — usable as a CI gate. See [docs/seo.md](docs/seo.md). |
| `omni seo report <target> [--json]` | Same checks as `seo check`, always exits 0 — informational only. |

All commands return a non-zero exit code on failure and print structured,
stable-coded diagnostics (e.g. `E-RESOLVE-NO-IMPLEMENTATION`,
`E-ADAPTER-TOOLCHAIN-MISSING`) to stderr — see
[docs/troubleshooting.md](docs/troubleshooting.md) for the full table.

## Example walkthrough: a static site

```sh
cd examples/hello-web
omni check                 # validate without building
omni build web              # writes dist/web/
omni dev web                 # serves it; prints READY web http://127.0.0.1:<port>
```

Open the printed URL: a real page with an `<h1>` from `web/app.yaml`, a
message from the component's default input, and `public/style.css`
applied. Press Ctrl-C to stop. `omni clean` removes `dist/`.

## Node.js backend walkthrough

```sh
cd examples/hello-node
omni check                 # validate without building
omni build backend          # writes dist/backend/, really invokes node
cat dist/backend/output.txt  # real captured output from the real script
```

`examples/hello-node` is structurally identical to `examples/hello-backend`
— same `omni.yaml` shape, same `svc.greeter` component, same
`backend/service.json` entry file — but its component declares
`implementations.backend.adapter: node-backend` and its source is
`service.js` instead of `service.py`. Requires a Node.js interpreter on
`PATH` (`node`, or `nodejs` on Debian/Ubuntu); `omni doctor` reports
exactly what's missing if it isn't found. See
[ADR 0012](docs/decisions/0012-node-backend-adapter.md).

## Full-stack walkthrough: web + Python backend

```sh
cd examples/hello-fullstack
omni dev
```

You'll see each target build, then a `READY <target> http://127.0.0.1:<port>`
line as it starts serving — `backend` first (a real, supervised Python
HTTP server process), then `web` (a static page whose `api_base` is baked
in at build time to match `backend`'s configured port). Open the `web`
URL: the page's own JavaScript fetches from the real backend and renders
its response — not a simulated call. Press Ctrl-C: `omni dev` stops the
backend process (a graceful signal first, a hard kill only if that
doesn't work within its grace period) and the static server, printing
what happened to each before exiting.

This example is the project's main proof point: one `omni.yaml`, two
targets, one declared API contract
(`examples/hello-fullstack/backend/api-contract.json`), checked for real
at three levels — HTTP (`crates/omni-cli/tests/fullstack_http.rs`),
process lifecycle (`crates/omni-cli/tests/dev_lifecycle.rs`), and a real
browser (`integration-tests/browser/`). See
[examples/hello-fullstack/README.md](examples/hello-fullstack/README.md)
for exactly what's automated vs. what that's still manual.

## Android walkthrough: a real, built APK

```sh
cd examples/hello-android
omni doctor          # shows exactly which SDK/JDK/Gradle will be used
omni build android    # genuinely invokes Gradle; writes dist/android/app-debug.apk
```

`dist/android/app-debug.apk` is a real, debug-signed Android package —
inspect it with `unzip -l` or Android's own `aapt2 dump badging` to see
its real package name, app label, and SDK versions. Requires a real
local Android SDK/JDK/Gradle (see
[Requirements and prerequisites](#requirements-and-prerequisites) and
[ADR 0011](docs/decisions/0011-android-gradle-adapter.md)); `omni doctor`
reports exactly what's missing if the build can't run. See
[examples/hello-android/README.md](examples/hello-android/README.md) for
exactly what's verified vs. not (no emulator/device install, no release
signing, no `.aab`).

## Adapter architecture and extension guide

An adapter connects OmniForge to one real toolchain. Every adapter
implements the same Rust trait:

```rust
pub trait Adapter {
    fn manifest(&self) -> &AdapterManifest;
    fn validate(&self, ctx: &BuildContext) -> Diagnostics;
    fn compile(&self, ctx: &BuildContext) -> CompileOutput;
    // Optional: start this target's build output as a long-lived dev
    // server. Default: not supported.
    fn dev_serve_plan(&self, report: &TargetReport, output_dir: &Path)
        -> Option<Result<DevServePlan, Diagnostic>>;
}
```

Three implementation styles exist today, and a new adapter can use
whichever fits its real toolchain:

- **In-process** (`web-static`): implement the trait directly in Rust,
  compiled into the `omni` binary, with no subprocess at all. Fastest,
  but means the adapter's logic is Rust code.
- **Out-of-process** (`python-backend`, via the generic, language-neutral
  `ProcessAdapter`): the adapter is a *separate process* speaking a
  versioned JSON protocol over stdin/stdout — see
  [docs/adapter-protocol.md](docs/adapter-protocol.md) for the exact
  wire format. `ProcessAdapter` itself has zero Python-specific code; a
  Node.js, Go, or any other out-of-process adapter reuses it unchanged,
  only supplying a different script/executable and manifest.
- **In-process, safely shelling out to a real external build tool**
  (`android-gradle`): the adapter's decision-making is in-process Rust
  (registered as `in-process/v1`, like `web-static`), but it genuinely
  invokes a real external tool (Gradle) that doesn't speak OmniForge's
  own protocol, via a new generic primitive —
  `omni_adapter_api::process::run_tool` — carrying forward the exact same
  no-shell/controlled-environment/bounded-timeout discipline
  `process::execute` already enforces for out-of-process adapters, just
  without the JSON request/response framing. See
  [ADR 0011](docs/decisions/0011-android-gradle-adapter.md).

**To add a new adapter today**, you need to:

1. Write an `adapter.yaml` manifest declaring `id`, `version`, `protocol`,
   and `capabilities` (languages, component kinds, targets, operations) —
   validated against [`specs/adapter.schema.json`](specs/adapter.schema.json).
2. Implement `validate`/`compile` — either in Rust (in-process) or as a
   script/binary speaking the wire protocol (out-of-process).
3. Optionally declare a `dev` block (see
   [docs/adapter-protocol.md](docs/adapter-protocol.md#dev-serve-capability-adapteryamls-optional-dev-block))
   if your target can run as a long-lived dev server.
4. Run it through the conformance checker
   (`omni_adapter_api::conformance::check_manifest_invariants`) — both
   shipped adapters' test suites call it; a new adapter should too.
5. Register it in `crates/omni-cli/src/registry.rs` (today: the explicit,
   reviewed list of adapters compiled into this binary — there is no
   third-party/dynamic loading yet, by design; see
   [docs/security-model.md](docs/security-model.md)).

There is currently no separate "Adapter SDK" crate beyond
`omni-adapter-api` itself — that's tracked as a near-term roadmap item
(see [ROADMAP.md](ROADMAP.md)) rather than claimed to exist today.

## Components and the API contract format

A **component** (`component.yaml`) declares a name, kind (`ui`,
`service`, `logic`, `library`, `tool`), a typed interface (inputs,
defaults, required-ness), and one implementation per target it supports.
The resolver checks every adapter capability (target, kind, operation)
*before* ever calling into an adapter — an adapter is never invoked for
something it didn't declare support for. A component may also declare
`composes`: named slots whose value is another component's fully
rendered, already-escaped output, letting components like a navbar or
footer be written once and embedded inside others — see
[ADR 0010](docs/decisions/0010-component-composition-via-slots.md) and
[`examples/navbar-demo`](examples/navbar-demo).

An **API contract** (`api-contract.json`) is a small, custom,
OpenAPI-*inspired* document — paths, HTTP methods, request/response
bodies as inline JSON Schema (draft-07) fragments, and per-status-code
responses. It is **not** an OpenAPI document (see
[Explicitly unsupported / planned](#explicitly-unsupported--planned)).
A target can declare `contract: path/to/api-contract.json` in
`omni.yaml`; when it does, `omni build`/`omni check` schema-validate that
file as part of building the target, and its bytes become part of the
build's cache key. The library also exposes reusable validators
(`ApiOperation::validate_request_body`, `ApiResponse::validate_body`) so
a real HTTP response or request body can be checked against the declared
schema — used by `crates/omni-cli/tests/fullstack_http.rs` against a
real running backend. See [docs/api-contract-format.md](docs/api-contract-format.md),
[docs/component-specification.md](docs/component-specification.md), and
[ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md).

## Building, developing, testing, and cleaning

```sh
omni build web                 # one target
omni build all                 # every enabled target, independently
omni build all --allow-partial # accept a target with no available adapter
omni dev                       # build + serve every enabled target, managed lifecycle
omni dev web backend --port web=5173 --port backend=9000
omni test                      # honest "no tests implemented" report today
omni package web                # dist/web.zip, from an already-built target
omni clean                     # remove dist/, safely scoped to the project root
```

See [docs/dev-workflow.md](docs/dev-workflow.md) for port configuration,
what actually runs during `omni dev`, and the distinction between
development and production behavior.

### Testing this repository (for contributors)

```sh
cargo build --workspace
cargo test --workspace      # 179 tests: unit + integration + adapter conformance + dev-process lifecycle + composition + node-backend + typescript-backend + go-backend + parallel-build + seo + adapter-validate + android-gradle (real Gradle build skipped unless OMNIFORGE_RUN_ANDROID_E2E=1 — see ADR 0011)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
bash scripts/verify.sh      # all of the above, plus end-to-end builds of all three examples
```

A separate, additional, real-browser suite (Playwright/Chromium, requires
Node.js) lives in `integration-tests/browser/` — see
[its README](integration-tests/browser/README.md). It is **not** run by
`cargo test`/`scripts/verify.sh`:

```sh
cd integration-tests/browser
npm install
npx playwright install chromium
npm test
```

## Troubleshooting

Every error prints a stable code (`E-...`) and, usually, a suggestion.
`omni check`/`omni doctor` catch most configuration and toolchain
problems before a build. The full table of documented codes and fixes is
[docs/troubleshooting.md](docs/troubleshooting.md).

## Contributing

Issues and pull requests are welcome — see [CONTRIBUTING.md](CONTRIBUTING.md)
for development setup, project ground rules (the core must stay
language-neutral; a feature isn't "Implemented" without a passing test;
new adapters must pass the conformance suite), and commit/PR expectations.
Please read [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), and report security
issues per [SECURITY.md](SECURITY.md) rather than as a public issue.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the high-level direction and
[docs/roadmap.md](docs/roadmap.md) for the full, detailed,
continuously-updated Implemented/Partial/Planned/Blocked breakdown that
backs every status claim in this README.

## License

MIT — see [LICENSE](LICENSE).

## Acknowledgements

OmniForge is built on top of (and grateful to) the broader Rust and
open-source ecosystem it depends on directly, including
[Cargo](https://doc.rust-lang.org/cargo/)/[Clap](https://docs.rs/clap)
(CLI parsing), [serde](https://serde.rs)/[serde_yaml](https://docs.rs/serde_yaml)
(configuration), [jsonschema](https://docs.rs/jsonschema) (schema
validation), [ctrlc](https://docs.rs/ctrlc) (cross-platform shutdown
signaling), and [Playwright](https://playwright.dev) (real-browser
testing). See `Cargo.lock` for the complete, exact dependency list.
