# ADR 0016: A real `windows-desktop` adapter — the first non-web/backend/Android target

## Status
Accepted.

## Context
`windows`, `linux`, and `macos` have been valid target *names* in
`omni.yaml` since this project's first release (`KNOWN_TARGETS`), but no
adapter existed for any of them — enabling one correctly produced an
honest `unsupported` result rather than a fake success. The assignment
driving this milestone named desktop platforms as a mandatory deliverable.
This project's own development machine is Windows, which makes `windows`
the only desktop target genuinely buildable *and* verifiable here without
a different OS. Rust (via `rustc`) is always present on any machine that
can build OmniForge itself — making it the one implementation language
that needs zero new toolchain installation for this adapter, unlike the
mandatory-language-adapter work (Go, TypeScript) that required installing
or resolving a separate toolchain first.

## Decision
`windows-desktop` (`crates/omni-windows-adapter/`) reuses the exact
*style* `android-gradle` established (ADR 0011): in-process Rust
(`protocol: in-process/v1`) that safely shells out to a real external
tool — here, `rustc`, via the same `omni_adapter_api::process::run_tool`
primitive Gradle invocations already use (no-shell, controlled-env,
bounded-timeout). It does **not** reuse `ProcessAdapter`/the out-of-
process wire protocol at all, because there is no separate adapter
*process* to speak that protocol to — the adapter's own decision-making
runs in the `omni` binary itself, same as `web-static`.

Scope, stated as plainly as `web-static`'s (ADR 0003) and
`android-gradle`'s: one project, one root `ui`-kind component, rendered
through the exact same fixed `{{field}}`-substitution template style as
every other template-based adapter (`rust_template.rs`, a direct sibling
of `omni-android-adapter`'s `kotlin.rs` — same algorithm, escaping a Rust
string literal instead of a Kotlin one) into a single real `.rs` file,
compiled by a real `rustc` into a real native Windows executable (a
genuine PE32+ binary, confirmed directly with `file`, not assumed from a
successful exit code), which is then genuinely **executed** — not merely
checked for existence — with its captured stdout kept as a real,
inspectable artifact (`output.txt`), the same "prove it actually runs"
discipline the backend adapters (`python-backend`, `node-backend`, etc.)
already use.

**No GUI window, no dialog, no windowing toolkit.** This proves native
Windows executable compilation and execution for the `windows` target
for real — not a fake platform claim — but a console application, not a
graphical one. A real GUI surface (e.g. a native message box via
`user32.dll`'s `MessageBoxW`, callable from pure Rust `std` via `extern
"system"` with zero new crate dependencies) is architecturally
compatible with this exact mechanism but was not attempted in this
milestone: a real GUI call blocks on user interaction, which cannot be
exercised by an automated test without either faking the interaction or
accepting a hung CI job — a real constraint, not an oversight to paper
over. Tracked as Planned.

A real, found-not-theorized gap this milestone closed: the first draft
of `validate()` only rendered the template and never actually invoked
`rustc`, so `omni check` reported success for Rust source `rustc` itself
would reject. Fixed by having `validate()` run a real
`rustc --emit=metadata` type-check (compiles enough to catch every
category of real compile error, writes nothing lasting — the temp
directory is discarded immediately) before ever claiming success; caught
by a dedicated test
(`crates/omni-windows-adapter/tests/compile.rs::
a_genuine_compile_error_fails_both_validate_and_compile`) that failed
honestly on the first run.

## Consequences
- OmniForge now has a working, tested adapter for a desktop target —
  the first non-`web`/`backend`/`android` target — proving the
  `Adapter` trait and `KNOWN_TARGETS` vocabulary generalize to desktop
  platforms, not just the three categories built so far.
- New schema `specs/windows-app.schema.json`
  (`omniforge.dev/windows-app/v1`), registered in
  `omni_core::schema::SchemaKind` alongside `WebApp`/`AndroidApp` —
  `app_name` + `root.component`/`root.inputs`, the same shape as the
  Android entry file.
- `linux`/`macos` desktop adapters remain unimplemented — this machine
  cannot build or verify either OS at all (not a missing-toolchain gap
  like Go was; a different-OS gap, same category this project already
  states honestly for iOS). A Rust-based adapter using the same
  mechanism is a plausible future design for both, cross-compiled or
  verified on the right CI runner, but not attempted here.
- `examples/hello-windows` is real, buildable, and its artifact was
  independently confirmed to be a genuine PE32+ executable (`file
  dist/windows/app/hello-windows.exe` → "PE32+ executable for MS Windows
  ... (console), x86-64") and run directly outside the adapter's own
  invocation, not merely trusted from a successful build report.
