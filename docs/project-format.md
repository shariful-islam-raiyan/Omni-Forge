# Project format: `omni.yaml`

Schema: `specs/project.schema.json` (`schema: omniforge.dev/project/v1`).
Loader: `crates/omni-core/src/loader.rs`.

## Shape

```yaml
schema: omniforge.dev/project/v1

project:
  name: hello-web          # ^[a-z][a-z0-9-]{1,63}$
  version: 0.1.0           # semver-ish: \d+\.\d+\.\d+(-pre)?

targets:
  web:
    enabled: true
    adapter: web-static    # required when enabled: true
    entry: web/app.yaml    # adapter-specific; optional, resolved safely against project root
    dev:
      port: 4280           # optional; omit to auto-pick a free port (see dev-workflow.md)
  android: { enabled: false }
  ios: { enabled: false }
  windows: { enabled: false }
  linux: { enabled: false }
  macos: { enabled: false }
  backend:
    enabled: false
    contract: backend/api-contract.json   # optional; validated as part of the build, see api-contract-format.md

components:
  directory: lib/components     # default shown

build:
  output: dist                  # default shown
  cache: true                   # default shown
  parallel: false                # default shown; stored but not yet acted on (see roadmap)

security:
  strict_adapter_permissions: true   # default shown; see security-model.md
  audit_dependencies: true           # default shown; not yet actively enforced (see roadmap)
```

Every top-level section except `schema`, `project`, and `targets` is
optional and defaults as shown above (`crates/omni-core/src/spec/project.rs`).
Per-target `contract` and `dev` are themselves optional additions to the
*same* `omniforge.dev/project/v1` schema (added in the "Unified Application
Runtime" milestone) — additive and fully backward compatible, so no schema
version bump was needed: an `omni.yaml` written before they existed still
validates unchanged, and omitting either on a given target keeps its old
behavior (no build-time contract check; `omni dev` auto-picks a free port).

## Validation

1. **Schema validation** (`specs/project.schema.json`) catches structural
   problems: unknown top-level keys, wrong types, malformed name/version.
2. **Schema version check** — `schema` must equal exactly
   `omniforge.dev/project/v1`; anything else is `E-PROJECT-SCHEMA-VERSION`.
   There is currently one supported version. The compatibility policy:
   a `v2` would be handled by adding a new `SchemaKind`/parser path and
   accepting both for a deprecation window, documented here when it
   happens — nothing speculative is implemented today.
3. **Semantic validation** (`semantic_checks` in `loader.rs`):
   - `E-PROJECT-UNKNOWN-TARGET` — a target key is not one of
     `KNOWN_TARGETS` (`web, android, ios, windows, linux, macos, backend`).
     Unknown targets are a hard error, not silently ignored, per rule 15 of
     the engineering brief: an unsupported target must never look like an
     accepted one.
   - `E-PROJECT-MISSING-ADAPTER` — a target has `enabled: true` but no
     `adapter` key.
   - `E-PROJECT-NAME` — empty project name (schema already enforces the
     pattern; this guards the case schema validation is bypassed by a
     direct `ProjectFile` construction, e.g. in tests).

All three categories are returned together as `OmniError::Invalid(Vec<Diagnostic>)`
so `omni check` can print every problem in one pass instead of stopping at
the first.

A target's `contract` is **not** checked here — it's a build-time
dependency, not a project-load-time one: `crates/omni-cli/src/orchestrator.rs::build_target`
resolves and schema-validates it (and feeds its bytes into that target's
build cache key) as part of `omni build`/`omni check`'s per-target
resolution pass, so an invalid contract fails that target's build with an
`E-API-CONTRACT-*`/`E-SCHEMA-API-CONTRACT` diagnostic rather than being
silently accepted at load time and only discovered later. See
[api-contract-format.md](api-contract-format.md).

## Path resolution

`components.directory` and `build.output` (and a target's `entry`) are
resolved with `omni_core::paths::safe_join`, which rejects absolute paths
and any `..` that would escape the project root — see
[security-model.md](security-model.md).

## Compatibility policy

- A project file's `schema` must match a version this CLI build knows
  about exactly; there is no fuzzy "best effort" parsing of an unknown
  version.
- Only one version (`v1`) exists today, so there is nothing to migrate
  from yet. When `v2` is introduced, this document will state which
  fields changed and whether `v1` projects keep working unmodified.
