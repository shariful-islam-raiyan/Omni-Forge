# Roadmap

This is a short, high-level summary. The authoritative, continuously
updated, item-by-item breakdown (Implemented / Partial / Planned /
Blocked, with the exact code and test backing each claim) lives in
[docs/roadmap.md](docs/roadmap.md) — read that for details; this file
exists so the big picture is visible without opening `docs/`.

## Where OmniForge is today

- **Core engine** (project/component/adapter loading, validation,
  dependency resolution, build caching, diagnostics) — implemented and
  tested, language-neutral.
- **Three real adapters for `web`/`backend`**: `web-static` (in-process,
  static sites), `python-backend` (out-of-process, a real Python
  interpreter), and `node-backend` (out-of-process, a real Node.js
  interpreter — see ADR 0012). The latter two are interchangeable per
  component for the `backend` target.
- **A unified `omni dev`** that builds and serves every enabled target at
  once, managing a real backend dev-server process with graceful
  shutdown.
- **A full-stack example** (`examples/hello-fullstack`) proving a web
  frontend and a Python backend can be built from one project file and
  talk to each other over a declared, validated API contract — checked at
  the HTTP level, the process-lifecycle level, and the real-browser level.
- **Component composition** (`composes` in `component.yaml`): one
  component's rendered output can be embedded inside another's — see
  `examples/navbar-demo` and ADR 0010.
- **A real Android adapter** (`android-gradle`): `omni build android`
  genuinely invokes a locally-discovered Gradle and produces a real,
  verified debug APK — see `examples/hello-android` and ADR 0011.
  Verified on Windows; macOS/Linux toolchain discovery is implemented but
  unverified.

See [README.md](README.md#current-implementation-status) for the
Implemented/Partial/Planned status table, and [docs/roadmap.md](docs/roadmap.md)
for the full detail.

## Where it's going

In rough priority order (not a committed schedule):

1. **Adapter SDK and conformance tooling** — make it genuinely
   straightforward for a third party to write a new language/target
   adapter against a documented, versioned protocol, with a conformance
   test suite to check against.
2. **Done: a second and third real backend-language adapter** —
   `node-backend` (Node.js) and `typescript-backend` (real `tsc`
   type-checking + compile), both reusing the existing `ProcessAdapter`
   mechanism unchanged, each with its own real end-to-end example
   (`examples/hello-node`, `examples/hello-typescript`); see
   [ADR 0012](docs/decisions/0012-node-backend-adapter.md)/
   [ADR 0013](docs/decisions/0013-typescript-backend-adapter.md). A Go
   adapter remains a candidate, but no Go toolchain exists on this
   project's own development machine to build and test one against yet.
3. **Parallel build scheduling** across independent targets — the
   dependency graph is already schedule-ready; no scheduler exists yet.
4. **Desktop and iOS targets** (Windows, Linux, macOS desktop; iOS) —
   Android now has a real adapter (`android-gradle`, above). The
   remaining native targets each need real toolchain research and a real
   sample app proving the packaged output actually runs before any
   adapter is claimed to support them; iOS additionally needs a
   macOS/Xcode environment this project doesn't have. Not started.
   Nearer-term for Android itself: emulator/device install, release
   signing, and `.aab` output.
5. **Third-party adapter trust/enablement policy** — today only adapters
   this repository ships and reviews can run; nothing is discovered or
   downloaded. A real trust model is a prerequisite for ever changing that.
6. **Real OpenAPI 3.1 support**, if/when a consumer actually needs it —
   the current API contract format is deliberately smaller and custom
   (see [ADR 0008](docs/decisions/0008-api-contract-is-a-custom-format.md)).

Nothing above is implemented by having a placeholder, a stub, or a
manifest entry with no working code behind it — see
[docs/roadmap.md](docs/roadmap.md)'s status definitions for what
"Implemented" actually requires here.
