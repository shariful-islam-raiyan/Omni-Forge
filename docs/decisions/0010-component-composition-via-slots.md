# ADR 0010: Component composition via named, pre-rendered slots

## Status
Accepted.

## Context
Before this milestone, `web-static` rendered exactly one component per
page: `omni.yaml`'s `web/app.yaml` names a single `root.component`, and
`ComponentManifest::dependencies` was only ever used by the resolver
(`crates/omni-core/src/resolver.rs`) to build the dependency graph and
order a build — resolving a component's dependencies never caused their
*output* to appear anywhere in the root component's rendered HTML. A
project with a `ui.navbar` and a `ui.page` component had no way to make
the page actually contain the navbar's markup short of copy-pasting the
navbar's template into the page's template by hand. This was documented
honestly (`docs/roadmap.md`'s "Component system" table: "neither shipped
adapter reads or emits [outputs/events]"), but it was still a real,
user-visible gap for a system whose stated goal is reusable components
(Navbar, Button, Header, Footer, …).

[ADR 0003](0003-template-only-rendering.md) deliberately rejected a "full
templating language (conditionals, loops, includes)" in favor of fixed
`{{field}}` substitution, specifically to keep the renderer's entire
security argument to two sentences and avoid becoming an unreviewed
string-execution mechanism. Any composition feature has to be judged
against that same bar — it must not reintroduce conditionals, loops, or a
general "take this string and run it as code" mechanism — rather than
treated as a reason to avoid composition altogether.

## Decision
Add exactly one new, bounded primitive: a component's `component.yaml` may
declare a `composes` map alongside `interface.inputs`:

```yaml
dependencies: [ui.navbar]   # required: see below
composes:
  nav:                       # slot name — usable as {{nav}} in this
                              # component's own template
    component: ui.navbar     # must appear in 'dependencies'
    inputs:                  # passed to ui.navbar exactly like a webapp
      brand: "My Site"        # manifest's root.inputs are passed to the root
```

Rendering (`crates/omni-web-adapter/src/lib.rs::render_component`) treats
`composes` as nothing more than a second source of `{{name}}` placeholder
values, alongside `interface.inputs`: for each slot, it recursively calls
the *exact same* rendering function on the named child component with the
slot's `inputs`, and inserts the child's fully-rendered HTML as that
placeholder's value. `template.rs::render` gained a two-variant `SlotValue`
(`Escaped` — HTML-escaped, as every plain input always was;
`Raw` — inserted verbatim) so a slot's pre-rendered HTML isn't
double-escaped, but a plain input's value always still is. There is still
no conditional logic, no loop construct, and no mechanism that accepts an
arbitrary string and executes it — `composes` only ever causes one more
recursive call to this same fixed renderer, which itself only ever
produces escaped text or more recursively-rendered HTML. The recursion
depth is bounded by the project's real component count, and:

- **Every `composes` target must already be a declared `dependencies`
  entry** (`crates/omni-core/src/components.rs::discover_components`,
  `E-COMPONENT-COMPOSE-UNDECLARED-DEPENDENCY`). This means composition
  reuses the resolver's existing, already-tested dependency graph and
  cycle detection (`E-RESOLVE-CYCLE`) instead of inventing a second one —
  a composition cycle is a dependency cycle, full stop.
- **A slot name may not collide with a declared input name**
  (`E-COMPONENT-COMPOSE-NAME-CONFLICT`) — both live in the same
  `{{name}}` placeholder namespace for that component's template.
- The adapter *also* keeps its own recursion-stack guard
  (`E-WEB-COMPOSE-CYCLE`) purely as defense in depth, in case the
  dependency-graph guarantee is ever bypassed by a future caller of
  `render_component` that doesn't go through normal discovery — not
  because a real cycle is currently reachable through the supported path.

This is backward compatible: `composes` is optional (`#[serde(default)]`,
not in `required` in `specs/component.schema.json`); every existing
component manifest and example (`hello-web`, `hello-backend`,
`hello-fullstack`) has none and renders exactly as before — proven by the
pre-existing test suite passing unchanged.

## Consequences
- A component can now genuinely be reused *inside* another component's
  rendered output — the actual "Navbar/Button/Header reusable component"
  use case the project's design docs describe — not merely be a build-time
  dependency that happens to also get built.
- `examples/navbar-demo` was extended into a real composition example
  (`ui.page` composing `ui.navbar`), with CI/`scripts/verify.sh` coverage.
- The security argument grows by one paragraph, not by a new risk class:
  a composed slot's value is never derived from anything other than this
  same renderer's own escaped or recursively-rendered output — see
  `template.rs`'s module docs and
  `crates/omni-web-adapter/tests/compile.rs::composed_child_component_is_rendered_and_escaped_in_place`,
  which specifically asserts a child component's unescaped-looking input
  still comes out escaped once spliced into the parent.
- Still **not** solved, and not claimed to be: multiple candidate
  implementations per target (`docs/roadmap.md`'s "Blocked" row — a slot
  still selects a component by exact name, with exactly one `web`
  implementation each), passing a parent's *own* input value through to a
  slot's input (today a slot's `inputs` are static YAML values, not
  `{{field}}`-interpolated from the parent's own inputs — a real
  limitation worth revisiting, not yet attempted here), and
  outputs/events (still unread by any adapter).
