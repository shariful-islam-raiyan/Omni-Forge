# Troubleshooting

Every error OmniForge prints has a stable code. This is not an exhaustive
list of every code in the source — it's the ones you're likely to hit.

| Code | Meaning | Fix |
|---|---|---|
| `E-PROJECT-NOT-FOUND` | No `omni.yaml` in the current directory or any parent. | Run inside a project, or `omni create <name>` first. |
| `E-SCHEMA-PROJECT` | `omni.yaml` failed JSON Schema validation. | The message includes the failing JSON pointer (e.g. `/schema`, `/targets/web/enabled`) — fix that field. |
| `E-PROJECT-SCHEMA-VERSION` | `schema:` isn't `omniforge.dev/project/v1`. | This build only supports `v1`; fix the value or upgrade OmniForge. |
| `E-PROJECT-UNKNOWN-TARGET` | A key under `targets:` isn't one of `web, android, ios, windows, linux, macos, backend`. | Fix the typo, or if you mean a real future target, note it's modeled but has no adapter yet (`web` and `backend` do have one). |
| `E-PROJECT-MISSING-ADAPTER` | A target has `enabled: true` but no `adapter:`. | Add `adapter: web-static` (or disable the target). |
| `E-COMPONENT-DUPLICATE` | Two `component.yaml` files declare the same `name`. | Rename one; component names must be unique project-wide. |
| `E-COMPONENT-UNSAFE-SOURCE` | An implementation's `source` path resolves outside the component's own directory. | Use a relative path that stays under the component directory. |
| `E-RESOLVE-MISSING-COMPONENT` | A component (root or dependency) named in config doesn't exist. | Check the name and that its `component.yaml` is where `components.directory` expects it. |
| `E-RESOLVE-NO-IMPLEMENTATION` | A component has no `implementations.<target>` entry. | The message lists which targets it *does* support — add the missing one or disable that target. |
| `E-RESOLVE-CAPABILITY` | The implementation's adapter doesn't declare this target, this component kind, or the `compile` operation. | Check `omni adapter list` for what the adapter actually supports. |
| `E-RESOLVE-CYCLE` | Component dependencies form a cycle. | The message prints the actual cycle, e.g. `a -> b -> a`. |
| `E-BUILD-ADAPTER-UNAVAILABLE` | The configured adapter isn't compiled into this CLI. | Run `omni adapter list`; today `web-static` and `python-backend` exist. |
| `E-WEB-MISSING-INPUT` | A component's required input has no value from `web/app.yaml` and no default. | Add it under `root.inputs` in the app manifest, or give the component a default. |
| `E-WEB-ASSET-MISSING` | A CSS/JS/asset file listed in `web/app.yaml`'s `public:` section doesn't exist under `public/`. | Create the file or remove it from the list. |
| `E-WEB-TEMPLATE-RENDER` | The template references `{{something}}` that isn't a declared input. | Either declare it in `interface.inputs`, or fix the typo in the template. |
| `E-CREATE-DIR-NOT-EMPTY` | `omni create <name>` was given a name whose directory already exists and has files in it. | Pick a different name, or empty the directory yourself first — `omni create` never deletes existing files. |
| `E-ADAPTER-TOOLCHAIN-MISSING` | None of `python-backend`'s launcher candidates (`python3`, `python`, `py`) could be started. | Install Python 3 and ensure one of those names resolves on `PATH`; `omni doctor` confirms which one. |
| `E-ADAPTER-TIMEOUT` | The adapter process didn't respond within its timeout and was killed. | Usually means the backend script itself is hanging — check what it's doing; the adapter's own inner timeout (`E-PYADAPTER-EXECUTION-TIMEOUT`) fires first for a slow *script*, so seeing this instead suggests the adapter process itself, not your script, is stuck. |
| `E-ADAPTER-PROTOCOL-MALFORMED` / `E-ADAPTER-PROTOCOL-EMPTY-RESPONSE` | The adapter process didn't produce a valid single-line JSON response. | This points at a bug in the adapter itself (or something printing extra output to stdout), not your project — please report it. |
| `E-PYADAPTER-MISSING-INPUT` | A `service` component's required input has no value from the backend entry file and no default. | Add it under `inputs` in `backend/service.json` (or whatever your `entry:` points to), or give the component a default. |
| `E-PYADAPTER-EXECUTION-FAILED` | Your `service.py` (or equivalent) exited non-zero. | The diagnostic's explanation includes the script's stderr tail. |
| `E-PYADAPTER-UNSAFE-PATH` | A component's resolved source path, the output directory, or the entry file resolves outside the project root. | This should not happen through normal `omni build` usage (the core already validates these) — if you see it, something unusual is going on with your paths or symlinks. |
| `E-API-CONTRACT-JSON` / `E-API-CONTRACT-PARSE` / `E-SCHEMA-API-CONTRACT` | A target's declared `contract:` file (`omni.yaml`) isn't valid JSON, doesn't schema-validate, or doesn't deserialize. | Only reported for a target that actually declares `contract: path/to/api-contract.json` — see [api-contract-format.md](api-contract-format.md). Fix the file; it's validated as part of `omni build`/`omni check` for that target now, not only by a hand-written test. |
| `E-DEV-UNKNOWN-TARGET` | `omni dev <name>` named a target that isn't in `omni.yaml`'s `targets:`. | Check the spelling, or omit the target list entirely to run every enabled target. |
| `E-DEV-PORT-CONFLICT` | Two or more targets are configured (via `omni.yaml`'s `dev.port` or `--port`) to use the *same* explicit port. | Give each target its own port, or unset one so it auto-picks a free port instead. Detected before anything is spawned — see [dev-workflow.md](dev-workflow.md). |
| `E-DEV-PORT-UNAVAILABLE` | `omni dev` could not even allocate an auto-picked free port for a target (very rare — generally means the OS is out of ephemeral ports). | Close other listening processes/retry; if persistent, something unusual is happening at the OS level. |
| `E-DEVSERVER-SPAWN-FAILED` / `E-DEVSERVER-READY-TIMEOUT` / `E-DEVSERVER-EXITED-BEFORE-READY` / `E-DEVSERVER-EXITED-EARLY` / `E-DEVSERVER-UNEXPECTED-OUTPUT` | A dev-serve-capable target's backend process failed to start or never reported readiness. | The diagnostic includes the process's stderr tail. This is the same family of problem `E-ADAPTER-TIMEOUT`/`E-PYADAPTER-EXECUTION-FAILED` describe for a *build*, just for `omni dev`'s long-running process instead. |
| `E-DEVSERVER-ARTIFACT-LAYOUT` | An adapter declares a `dev` capability, but this build's output doesn't follow the required "runnable-package convention" (exactly one file under `app/` plus a `run-config.json`). | This indicates an adapter bug, not a project misconfiguration — see [adapter-protocol.md](adapter-protocol.md#dev-serve-capability-adapteryamls-optional-dev-block). |
| `E-PACKAGE-NOT-BUILT` | `omni package <target>` was run before `omni build <target>` — there's no `dist/<target>/` to archive. | Run `omni build <target>` (or `omni build all`) first. |
| `E-PACKAGE-UNKNOWN-TARGET` | `omni package <name>` named a target that isn't a recognized OmniForge target name. | Check the spelling, or use `all` to package every enabled target. |
| `E-PACKAGE-IO` | A filesystem error occurred while reading the build output or writing the zip archive. | The message includes the underlying OS error; check disk space and permissions on the project's `dist/` directory. |

## General approach

1. Run `omni check` before `omni build` — it catches config/resolution
   problems without needing a full build or touching `dist/`.
2. Diagnostics that reference a file include its path; diagnostics that
   reference a component or target are tagged with `with_component`/
   `with_target` and show up as such in `--json` output
   (`omni check --json`, `dist/build-report.json`).
3. `omni doctor` tells you whether the problem is "my project is
   misconfigured" vs. "a tool an adapter needs isn't installed." For
   `web-static`, nothing external is required, so `doctor` failing there
   would mean something unusual about your Rust toolchain. For
   `python-backend`, `doctor` tries `python3`, `python`, and (on Windows)
   `py` before reporting it missing — if all three are genuinely absent,
   install Python 3.
