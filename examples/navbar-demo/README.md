# navbar-demo

A minimal example proving component **composition**: one component's
rendered output embedded inside another's, via the `composes` field added
in [ADR 0010](../../docs/decisions/0010-component-composition-via-slots.md).

## What's here

- `lib/components/ui.navbar/` — a small, standalone, reusable `ui`
  component with no knowledge of anything that embeds it.
- `lib/components/ui.page/` — declares `ui.navbar` as a `dependencies`
  entry and composes it into a `nav` slot:

  ```yaml
  dependencies:
    - ui.navbar
  composes:
    nav:
      component: ui.navbar
      inputs:
        brand: "My Site"
  ```

  Its own template (`page.html.tmpl`) then uses `{{nav}}` exactly like any
  other placeholder — the only difference from a plain input is that its
  value is the real, already-rendered-and-escaped HTML of `ui.navbar`, not
  a scalar.
- `web/app.yaml` sets `ui.page` (not `ui.navbar`) as the page's root
  component.

## Building it

```sh
cd examples/navbar-demo
omni check
omni build web
omni dev web
```

Open the printed URL: a real navigation bar (rendered by `ui.navbar`,
unaware it's being embedded) followed by the page's own heading and body
(rendered by `ui.page`). View source — there is exactly one `<nav>`
element, produced once, not duplicated markup.

## What this does and doesn't prove

- **Does**: a component can be written once and reused, unchanged, inside
  another component's output — the core "Navbar/Button/Header as a
  reusable building block" use case.
- **Doesn't**: pass a parent's *own* input values through to a slot's
  inputs dynamically. `ui.page`'s `composes.nav.inputs.brand` is a static
  string in `component.yaml`, not `{{something}}`-interpolated from
  `ui.page`'s own `heading`/`body` inputs — see ADR 0010's Consequences
  section for exactly what's left.
