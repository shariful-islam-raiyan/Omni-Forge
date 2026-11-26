# ADR 0011: `android-gradle` — a real, in-process adapter that shells out to Gradle

## Status
Accepted.

## Context
Before this milestone, `android` was a valid target *name* in
`omni.yaml` (the schema never restricted target names) with zero adapter
behind it — enabling it correctly failed with
`E-RESOLVE-NO-IMPLEMENTATION`/`E-BUILD-ADAPTER-UNAVAILABLE`, never a fake
success (see `docs/roadmap.md`'s prior "Planned" entry). This milestone's
goal was a genuinely real next platform adapter, not a folder of
Android-looking files: a real Gradle invocation producing a real,
installable APK, verified as such.

This was investigated, not assumed: this project's own development
machine was checked for a real, usable Android toolchain (Android Studio
with an SDK — `platforms/android-36`, `build-tools/` up to `37.0.0` — a
bundled JDK, and Gradle already cached via `~/.gradle/wrapper/dists`), and
a minimal Kotlin/AGP project was built by hand first, outside any Rust
code, specifically to find a real, working version combination before
writing the adapter against it (see "Verified recipe" below). Two real,
non-obvious Windows-specific failures were found and fixed this way:

1. **Kotlin/Java JVM target mismatch.** Without an explicit
   `compileOptions`/`kotlinOptions.jvmTarget`, Kotlin compiles against
   whatever JDK actually runs Gradle (21, via Android Studio's bundled
   JBR) while Java defaults to 1.8, and AGP refuses the inconsistency.
   Fixed by pinning both to JVM 17 explicitly in the generated
   `app/build.gradle.kts`.
2. **`\\?\`-prefixed paths break Gradle's own path canonicalization.**
   `omni_core::loader::find_project_root` returns a canonicalized,
   extended-length (`\\?\`-prefixed) path on Windows — correct and
   necessary for `omni-core`'s own containment checks, but Gradle's
   Java-side `File.getCanonicalPath()` throws a raw
   `java.io.IOException: Invalid file path` if that prefix is ever passed
   to it as a `--project-dir` *string argument*. Fixed by never passing
   `--project-dir` at all: the generated project directory is set as the
   child process's actual working directory instead (a real `CreateProcess`
   call, which handles `\\?\` paths natively), and Gradle discovers its
   own project from the cwd, exactly as it would for a developer running
   `gradle` by hand inside the project folder.

## Decision

### Architecture: in-process Rust that shells out to a real build tool
Two adapter styles already existed: fully in-process with no subprocess
(`web-static`) and out-of-process adapters that speak OmniForge's own
JSON wire protocol (`python-backend`, via `ProcessAdapter`). Android fits
neither: the *decision-making* (what project to generate, how to invoke
Gradle, how to interpret its result) is naturally Rust code compiled into
`omni` (in-process, like `web-static`), but producing a real APK
genuinely requires invoking a real external build tool that does not and
should not need to speak OmniForge's own protocol. `android-gradle` is
registered as `in-process/v1` (its `Adapter::validate`/`compile` are
direct Rust code, not a subprocess satisfying the adapter wire protocol)
and internally calls a new, generic primitive —
`omni_adapter_api::process::run_tool` — that carries forward every safety
property `process::execute` already enforces for `python-backend`
(no shell, `Command::new(exe).args(..)` only, controlled/allowlisted
environment via `controlled_env_with`, enforced timeout via poll-then-kill)
without assuming the child speaks JSON back. `web-static` or a future
in-process adapter can reuse `run_tool` the same way without becoming an
`out-of-process/v1` adapter.

### Toolchain discovery (`omni_android_adapter::toolchain`)
Unlike `python3` (a single interpreter, found via a short alias list on
`PATH`), Android's toolchain is normally installed by Android Studio into
locations that are real and usable but not `PATH`-advertised:

- **SDK**: `ANDROID_HOME`/`ANDROID_SDK_ROOT` if set and valid (contains
  `platforms/` and `build-tools/`); else the per-OS default Android
  Studio install location (`%LOCALAPPDATA%\Android\Sdk` on Windows,
  `~/Library/Android/sdk` on macOS, `~/Android/Sdk` on Linux).
- **JDK**: `JAVA_HOME` if valid; else a `java` found on `PATH`; else (
  Windows only) Android Studio's bundled JBR at its documented default
  install path.
- **Gradle**: `GRADLE_HOME`/a plain `gradle` on `PATH`; else the
  newest-looking distribution already extracted into Gradle's own
  wrapper cache (`~/.gradle/wrapper/dists/*/*/gradle-*/bin/gradle`) —
  this adapter never downloads Gradle itself; it reuses one that running
  any Gradle wrapper (Android Studio itself does this) has already
  cached. "Newest-looking" is a lexical sort of distribution directory
  names, a pragmatic heuristic, not a real semver comparison.

A missing piece is `E-ANDROID-TOOLCHAIN-MISSING`, naming exactly what's
missing, surfaced by `compile()` and (precisely, not via the generic
`capabilities.requires` loop other adapters use) by `omni doctor`.

### `validate()` stays fast and offline; `compile()` does the real build
`validate()` parses/schema-validates the entry file, resolves the root
component, checks declared inputs, and renders the Kotlin template — all
in-process, no toolchain required. Only `compile()` detects the toolchain
and invokes Gradle. This differs from `python-backend`, whose
`validate()` *does* invoke the real toolchain — but that's an artifact of
`ProcessAdapter` unifying validate/compile through one subprocess call,
not a rule every adapter must follow. `omni check` for `android` is
therefore fast even without a local Android install; `omni doctor` (or a
real `omni build android`) is how a missing toolchain is actually
surfaced.

### Component template: the same fixed-substitution discipline, a different escaper
A component's `implementations.android.source` is a tiny template file
(e.g. `message.kt.tmpl`) whose entire rendered content becomes a Kotlin
string literal's value (`crates/omni-android-adapter/src/kotlin.rs`):
only `{{field}}` placeholders, every placeholder must be a declared
input, and every substituted value is escaped for safe inclusion in a
Kotlin string (`\`, `"`, and — unlike HTML escaping — `$`, since an
unescaped `$` would trigger Kotlin's own string-template interpolation;
see the `a_dollar_sign_cannot_trigger_kotlin_string_interpolation` test).
No logic, no loops, no conditionals — the same scope ADR 0003 drew for
HTML, redrawn for Kotlin. A template file's own trailing newline (nearly
universal in authored text files) is trimmed before rendering, since the
entire result is spliced into one single-line string literal, where a
raw newline would be invalid Kotlin syntax, not meaningful content.

### Verified recipe
Gradle **8.14.3** (and, separately re-verified, **9.3.1** — both already
cached on this machine), Android Gradle Plugin **8.7.3**, Kotlin Gradle
Plugin **2.0.21**, `compileSdk`/`targetSdk` **36**, `minSdk` **24**, JVM
target **17**. These are pinned literals in
`crates/omni-android-adapter/src/project.rs`, not user-configurable in
this milestone (`min_sdk`/`target_sdk`/`compile_sdk` *are* configurable
per-project via the `android-app` entry schema; the plugin versions are
not) — a deliberate, honest scope limit, not an oversight.

### Scratch project location, not inside `dist/`
The generated Gradle project lives under
`<project_root>/.omniforge/android-build/<target>/`, not inside
`ctx.output` (`dist/android/`): `ctx.output` is the real, cacheable build
output (content-addressed caching copies it wholesale), and a Gradle
project's own multi-megabyte `build/` tree is Gradle's own incremental
cache, not a OmniForge build artifact. Only the final, verified
`app-debug.apk` and a small `build-manifest.json` are copied into
`dist/android/`. The scratch directory is deliberately *not* wiped
between builds, so Gradle's own incremental build state (dependency
resolution, compiled classes) speeds up repeated builds the same way it
would for a developer running Gradle by hand in that folder.

### Artifact verification
A claimed-successful Gradle build is not trusted on its exit code alone:
the adapter opens the produced `app-debug.apk` as a real zip archive
(`zip::ZipArchive`) and confirms a nonempty `AndroidManifest.xml` entry
exists before copying it into `dist/` and reporting success — the same
"never trust a claimed success without re-reading the artifact"
philosophy `web-static`/`python-backend` already follow.

## Consequences
- `omni build android` now performs a real build: verified end-to-end on
  this project's own development machine, both as a crate-level
  integration test (`crates/omni-android-adapter/tests/compile.rs::
  builds_a_real_apk_end_to_end`, which genuinely invokes Gradle and
  asserts on the real APK) and via the real compiled CLI building
  `examples/hello-android` (`omni build android` → a real
  `dist/android/app-debug.apk`, inspected with `aapt2 dump badging` and
  confirmed to report the declared package name, app label, and SDK
  versions).
- **Not verified on macOS or Linux.** This project's development machine
  is Windows; the macOS/Linux toolchain-discovery default paths in
  `toolchain.rs` are implemented the same way Android Studio documents
  its own default install locations, but have not been exercised
  end-to-end here. `android-gradle`'s Windows support is Implemented and
  verified; its macOS/Linux support is Implemented but unverified — see
  `docs/roadmap.md`, which states this distinction explicitly rather than
  claiming blanket "cross-platform" support.
- **Not implemented, and not claimed**: emulator/device install or
  launch (no device lifecycle management exists), release signing, `.aab`
  output, any UI beyond one `Activity`/one `TextView`, and a second
  candidate AGP/Gradle/Kotlin version (only the one verified combination
  above is used — no version negotiation). All tracked as Planned in
  `docs/roadmap.md`, not silently implied to exist.
- CI: a new, separate `android` job (matching the existing `browser-e2e`
  job's isolation principle — a toolchain-availability issue there must
  never block the core Rust gate) was added to
  `.github/workflows/ci.yml`. Whether GitHub-hosted runners' preinstalled
  Android SDK/Gradle/Java combination actually satisfies this adapter's
  pinned AGP/Kotlin/compileSdk versions was **not** verified as part of
  this milestone (no access to actually run GitHub Actions from this
  environment) — if the toolchain is present but an unexpected
  version-compatibility error occurs there, that is a real finding to
  triage, not evidence this adapter is broken on a machine where it has
  been verified directly.
