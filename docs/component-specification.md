# Component manifest: `component.yaml`

Schema: `specs/component.schema.json` (`schema: omniforge.dev/component/v1`).
Discovery: `crates/omni-core/src/components.rs`.

## Shape

```yaml
schema: omniforge.dev/component/v1

name: ui.home            # ^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*)+$ — namespaced, e.g. "ui.home"
version: 1.0.0           # semver-ish
kind: ui                 # ui | logic | service | library | tool

dependencies:            # optional; other component names this one needs
  - ui.layout

interface:
  inputs:
    title:
      type: string        # string | boolean | number
      required: true
    message:
      type: string
      required: false
      default: "..."
  outputs: {}             # same shape as inputs; modeled, not yet consumed by web-static
  events: {}               # named, empty objects today; modeled, not yet consumed

implementations:
  web:
    adapter: web-static
    source: ./home.html.tmpl   # resolved relative to this manifest's directory
```

## Component kinds

| Kind | Meaning | Executable today? |
|---|---|---|
| `ui` | Renders to a target's UI | Yes, via `web-static` |
| `service` | A backend service boundary | Yes, via `python-backend` |
| `logic` | Pure/business logic, no UI | Modeled, no adapter executes it yet |
| `library` | Shared code with no independent output | Modeled, no adapter executes it yet |
| `tool` | A build-time helper | Modeled, no adapter executes it yet |

Modeling all five now (rather than only the ones with adapters) keeps the
schema stable as adapters for the other kinds are added, without implying
any of them currently do something they don't — see `docs/roadmap.md`.

### `service` components and the `python-backend` adapter

A `service`-kind component's `implementations.backend.source` is a Python
script invoked as `<python> <script.py> <run-config.json>`, where
`run-config.json` is this component's merged, validated inputs as a flat
JSON object (declared defaults, overridden by the target's entry file).
The script is expected to run to completion (it is not treated as a
long-running server) and exit; its captured stdout and a copy of itself
become build artifacts under `dist/backend/` — see
[adapter-protocol.md](adapter-protocol.md) for exactly what the adapter
sends it and [caching.md](caching.md) for how its output is cached.

Example (the full version lives in `examples/hello-backend`):

```yaml
schema: omniforge.dev/component/v1
name: svc.greeter
version: 1.0.0
kind: service
interface:
  inputs:
    greeting:
      type: string
      required: false
      default: "Hello from the OmniForge python-backend adapter."
implementations:
  backend:
    adapter: python-backend
    source: ./service.py
```

```python
# service.py
import json, sys
config = json.load(open(sys.argv[1])) if len(sys.argv) > 1 else {}
print(config.get("greeting", "default"))
```

### The `backend` target's entry file is adapter-owned, not core-owned

`omni-core` resolves a target's `entry` path safely (`safe_join`) but
**never parses its contents** for any target — only the adapter does.
`web-static`'s entry file (`web/app.yaml`) is YAML because `web-static`
is Rust code that already has a YAML parser available. `python-backend`'s
entry file (e.g. `backend/service.json`, schema
`omniforge.dev/backend-app/v1`) is plain JSON instead, specifically so the
adapter can stay standard-library-only (no PyYAML dependency to install):

```json
{
  "schema": "omniforge.dev/backend-app/v1",
  "service": "svc.greeter",
  "inputs": { "greeting": "Hello from hello-backend!" }
}
```

`service` names which resolved component is the build's primary target
(defaulting to the last-resolved component if omitted); `inputs` overrides
that component's declared defaults. This format is **not** validated
against a JSON Schema in `specs/` — it's parsed and checked by
`adapters/python-backend/adapter.py` itself (`read_entry()`), since no
other part of the system needs to understand it. A different
out-of-process adapter is free to choose an entirely different entry
format; there is no cross-adapter standard for this file, by design.

## Discovery and validation (`components.rs::discover_components`)

1. Walks immediate subdirectories of `components.directory`; each one with
   a `component.yaml` is a candidate. Order is sorted by directory name for
   determinism, and the result is re-sorted by component name.
2. YAML parse failure → `E-COMPONENT-YAML`. Schema failure →
   `E-COMPONENT-SCHEMA-*` codes per field. Unsupported `schema` version →
   `E-COMPONENT-SCHEMA-VERSION`.
3. Every `implementations.<target>.source` is resolved with `safe_join`
   against the component's own directory; an escaping path is
   `E-COMPONENT-UNSAFE-SOURCE` and excludes that component from the
   result (it does not abort discovery of other components).
4. Duplicate `name` across two manifests → `E-COMPONENT-DUPLICATE`; the
   second one encountered (in sorted order) is excluded.

A manifest with any error is excluded from the returned list rather than
aborting the whole discovery pass, so `omni check` can report every
problem in one run.

## Implementation selection

A component may declare at most one implementation per target key (the
schema's `implementations` map is keyed by target name, so two
implementations for the same target in one manifest is a structural
impossibility, not a runtime tie-break). Cross-target selection is by
target name only: building `web` uses `implementations.web` and ignores
any other target's entry. There is currently no "pick the best of several
candidates for the same target" rule to document, because the format does
not allow more than one.

If a component has no entry for the requested target at all, resolution
fails with `E-RESOLVE-NO-IMPLEMENTATION`, naming the component, the
target, and the targets it *does* have an implementation for — see
[resolver.md](resolver.md).

## Dependencies

`dependencies` is a flat list of other component names. The resolver
(`omni_core::resolver::resolve`) treats these as "must also be resolved
for the same target" and uses them to build the dependency graph; a
dependency that doesn't exist, has no implementation for the target, or
whose adapter lacks the right capability fails resolution with the same
diagnostics as if it were requested directly. Cycles are detected and
reported as `E-RESOLVE-CYCLE` with the full cycle path.
