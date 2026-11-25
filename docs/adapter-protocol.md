# Adapter protocol

There are two separate things in this repository worth not confusing:

1. **The `Adapter` trait** (`crates/omni-adapter-api/src/adapter.rs`) — the
   common interface `omni-cli`'s orchestrator calls, regardless of how an
   adapter is actually implemented.
2. **The wire protocol** (`crates/omni-adapter-api/src/protocol.rs`) — the
   versioned JSON message shapes an *out-of-process* adapter speaks over
   stdin/stdout. **This is real and in use**: `python-backend`
   (`adapters/python-backend/`) is a separate Python process that OmniForge
   spawns and exchanges exactly this protocol with, via the execution
   engine in `crates/omni-adapter-api/src/process.rs`.

`web-static` implements the `Adapter` trait directly, in-process (no wire
protocol involved). `python-backend` is wrapped in a `ProcessAdapter`
(also implementing `Adapter`), which speaks the wire protocol to a
subprocess. The orchestrator calls `.validate()`/`.compile()` on a
`Box<dyn Adapter>` either way — it does not know or care which kind it has.

## The `Adapter` trait (what every adapter implements)

```rust
pub trait Adapter {
    fn manifest(&self) -> &AdapterManifest;
    fn validate(&self, ctx: &BuildContext) -> Diagnostics;
    fn compile(&self, ctx: &BuildContext) -> CompileOutput;
    // Default impl returns `None`; see "Dev-serve capability" below.
    fn dev_serve_plan(&self, report: &TargetReport, output_dir: &Path)
        -> Option<Result<DevServePlan, Diagnostic>>;
}
```

- `manifest()` returns the adapter's identity, declared capabilities
  (languages, component kinds, targets, operations, external `requires`),
  and protocol tag — `in-process/v1` for `web-static`, `out-of-process/v1`
  for `python-backend`.
- `validate()` must not write any files; it returns diagnostics only. For
  an out-of-process adapter this still means spawning the process (there's
  no other way to ask it anything), just with `operation: "validate"`.
- `compile()` must only write inside `ctx.output`. For `ProcessAdapter`,
  the orchestrator additionally re-verifies every artifact the adapter
  process *claims* to have written actually exists and is readable before
  trusting the result (`E-BUILD-ARTIFACT-VERIFY` if not) — an
  out-of-process adapter's say-so is never sufficient on its own.

`BuildContext` is built by the orchestrator from already-validated,
already-resolved data: the project root, the target name, the target's
entry file path (if any), the output directory, and the list of
`ResolvedComponent`s the resolver chose for this target. An adapter never
sees unvalidated project configuration.

## Capability checks (before any operation runs)

The resolver (`omni_core::resolver::resolve`) checks, per component,
*before* any adapter method is called:

- the referenced adapter id exists (`E-RESOLVE-NO-ADAPTER`);
- that adapter declares the requested target (`E-RESOLVE-CAPABILITY`);
- that adapter declares the component's kind (`E-RESOLVE-CAPABILITY`);
- that adapter declares the `compile` operation (`E-RESOLVE-CAPABILITY`).

An adapter is never invoked for a target/kind/operation it didn't declare.
`python-backend` declares exactly `validate` and `compile` — nothing else —
because that's exactly what its script implements; sending it any other
operation (`package`, `test`, `run`, ...) gets a structured
`E-PYADAPTER-UNSUPPORTED-OPERATION` response, not a crash or a silent
no-op (see `crates/omni-cli/tests/python_backend_adapter.rs::
unsupported_operation_is_rejected_by_the_real_adapter`).

## Dev-serve capability (`adapter.yaml`'s optional `dev` block)

Added in the "Unified Application Runtime" milestone (see
[ADR 0009](decisions/0009-unified-dev-process-lifecycle.md)) so `omni dev`
can start an adapter's build *output* as a real, long-lived process —
deliberately **not** a new wire-protocol operation (`compile` stays
bounded, run-to-completion; the `Operation` enum's reserved `Run`/`Debug`/
`Inspect` variants are still unimplemented).

```yaml
# adapters/python-backend/adapter.yaml
dev:
  args: ["--serve", "{port}"]      # appended after <entry script> <config file>
  ready_stdout_prefix: "LISTENING" # first stdout line must start with this
  ready_timeout_ms: 10000
  stop_grace_ms: 3000              # how long to wait for a graceful stop
```

If a manifest declares `dev`, `ProcessAdapter::dev_serve_plan` (generic —
no Python-specific code) requires that build's artifacts follow the
**runnable-package convention**: exactly one file under `app/` (the entry
script) and a `run-config.json` at the output root — exactly what
`python-backend`'s `write_artifacts` already produces for every build,
regardless of dev-serving. `{port}` in `dev.args` is substituted by the
caller (`omni-cli::devrun`) once a concrete port is chosen; the plan itself
is built before any port is known. The resolved launcher (`python3`/
`python`/`py`, same resolution as a normal build) runs
`<launcher> <app-entry> <run-config.json> <dev.args with {port} filled in>`,
and readiness/stop are handled by `omni_adapter_api::devserver::ManagedProcess`
(spawn + stdout readiness line + bounded timeout; `stop()` attempts a
graceful signal — `SIGTERM` on Unix, `CTRL_BREAK_EVENT` on Windows — before
a hard kill).

A future out-of-process adapter that packages a runnable backend can
follow the same `app/<entry>` + `run-config.json` layout to get dev-serve
support with zero `omni-cli`/`omni-adapter-api` changes — this is a
packaging convention any adapter can opt into, not something hardcoded to
`python-backend`'s name or language. `web-static` (in-process) does not
declare `dev`; its "dev serving" is the CLI's own static file server, a
different mechanism entirely (see `docs/dev-workflow.md`).

## The wire protocol (real, in use by `python-backend`)

`protocol::PROTOCOL_VERSION = "omniforge.dev/adapter-protocol/v1"`.

```jsonc
// Request: one line of JSON written to the child's stdin, then stdin is closed.
{
  "protocol": "omniforge.dev/adapter-protocol/v1",
  "request_id": "1234-567890",
  "operation": "compile",        // only "validate" and "compile" are sent today
  "project_root": "/abs/path",
  "target": "backend",
  "component": "svc.greeter",     // the last-resolved ("primary") component, if any
  "source_files": ["/abs/path/lib/components/svc.greeter/service.py"],
  "entry_path": "/abs/path/backend/service.json",
  "output_dir": "/abs/path/dist/backend",
  "config": {
    "components": [
      {
        "name": "svc.greeter",
        "version": "1.0.0",
        "kind": "service",
        "interface": { "inputs": { "greeting": { "type": "string", "required": false, "default": "..." } }, "outputs": {}, "events": {} },
        "source_path": "/abs/path/lib/components/svc.greeter/service.py"
      }
    ]
  },
  "timeout_ms": 30000
}
```

```jsonc
// Response: one line of JSON read from the child's stdout.
{
  "protocol": "omniforge.dev/adapter-protocol/v1",
  "request_id": "1234-567890",   // must echo the request's id exactly
  "status": "ok",                 // ok|error
  "diagnostics": [ /* Diagnostic objects, same shape as everywhere else */ ],
  "artifacts": ["app/service.py", "run-config.json", "output.txt", "run.sh", "run.bat", "build-manifest.json"]
}
```

`entry_path` is resolved to an absolute path by the core (safely, via
`safe_join`), but its *contents* are never parsed by `omni-core` — only by
the adapter. `python-backend`'s entry file (e.g. `backend/service.json`) is
plain JSON with its own schema (`omniforge.dev/backend-app/v1`, documented
in `docs/component-specification.md`), chosen specifically so the adapter
needs no YAML library and stays stdlib-only. A future adapter in a
different language is free to use a different entry format; the core does
not and should not standardize this across adapters.

## Rules the protocol follows (enforced by `crates/omni-adapter-api/src/process.rs`)

1. **No shell.** Every spawn is `Command::new(executable).arg(script_path)`
   — an explicit executable name/path and an explicit argument array,
   never a shell string.
2. **Machine-readable messages stay on stdout/stdin only**; all
   human-readable logging goes to stderr, so nothing an adapter prints for
   debugging can corrupt the protocol stream. `python-backend`'s `log()`
   helper writes to `sys.stderr` exclusively.
3. **Controlled working directory.** The child's cwd is always the
   project root the core already validated.
4. **Controlled environment.** The child does **not** inherit the parent
   process's full environment. `execute()` calls `.env_clear()` and passes
   only a fixed allowlist (`PATH`, `SystemRoot`, `SystemDrive`, `TEMP`,
   `TMP`, `HOME`, `USERPROFILE`) — enough for a standard language runtime
   to start, nothing from the parent's own secrets or configuration.
5. **Enforced timeout.** The parent spawns reader threads for stdout/
   stderr and polls the child with `try_wait()` in a loop; if the deadline
   passes before the child responds, it is killed (`Child::kill()`) and
   reaped, and the diagnostic is `E-ADAPTER-TIMEOUT` — a hung or malicious
   adapter process cannot block a build indefinitely.
6. **Protocol discipline is checked, not assumed.** `execute()` rejects, as
   a `Diagnostic` rather than a panic or an assumed success:
   - no response line at all (`E-ADAPTER-PROTOCOL-EMPTY-RESPONSE`);
   - a response line that isn't valid JSON matching `AdapterResponse`
     (`E-ADAPTER-PROTOCOL-MALFORMED`);
   - a `protocol` field that doesn't match `PROTOCOL_VERSION` exactly
     (`E-ADAPTER-PROTOCOL-VERSION`);
   - a `request_id` that doesn't match what was sent
     (`E-ADAPTER-PROTOCOL-REQUEST-ID-MISMATCH`);
   - an artifact path in a successful response that resolves outside the
     output directory (`E-ADAPTER-UNSAFE-ARTIFACT-PATH`) — the adapter's
     own output directory containment is defense-in-depth, not the only
     check.
7. **Toolchain resolution has no silent failure mode.** If none of an
   adapter's launcher candidates (e.g. `python3`, `python`, `py`) can be
   started, the result is `E-ADAPTER-TOOLCHAIN-MISSING` naming every
   candidate tried — never a build that silently does nothing.

## Trust and enablement policy for third-party adapters (still planned)

**Not implemented.** `python-backend` is not a counterexample to this —
its script is embedded in the `omni` binary at compile time
(`crates/omni-cli/src/adapters/python_backend.rs`) and was reviewed as
part of this repository, exactly like `web-static`'s Rust code. Only
adapters `crates/omni-cli/src/registry.rs` explicitly constructs ever run;
nothing is discovered from disk or downloaded. A *third-party* adapter —
one this repository didn't ship — would need an explicit enablement step
(e.g. listing it by exact id and version in `omni.yaml` or a lockfile-like
file) before OmniForge would spawn it; that mechanism does not exist yet.
See `docs/security-model.md`.

## Conformance

`omni_adapter_api::conformance::check_manifest_invariants(&dyn Adapter)`
checks an adapter's manifest for internal consistency (non-empty id/
version/targets/operations, only known operation/kind names, a known
protocol tag: `in-process/v1` or `out-of-process/v1`). Both shipped
adapters' test suites call it:
`crates/omni-web-adapter/tests/compile.rs::adapter_manifest_is_conformant`
and `crates/omni-cli/tests/python_backend_adapter.rs::
adapter_manifest_is_conformant`. Any future adapter should do the same.
