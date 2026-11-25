# ADR 0003: Template substitution only, no embedded script engine

## Status
Accepted.

## Context
Section 10 of the original spec is explicit: "do not execute arbitrary
source code in a simplistic, unsafe string-evaluation mechanism" and
"prefer a clear and limited contract." A component needed *some* way to
turn declared inputs into HTML. The candidates were: (a) embed a JS engine
and let components be real `.js` files, (b) a full templating language
(conditionals, loops, includes), or (c) fixed `{{field}}` substitution.

## Decision
(c). `crates/omni-web-adapter/src/template.rs` recognizes only
`{{identifier}}`, requires every identifier to be a declared component
input (undeclared placeholders are a build error, not a silent no-op), and
HTML-escapes every substituted value. There is no conditional logic, no
loops, no includes, and no mechanism that takes a string and executes it.

## Consequences
- The entire security argument for component rendering fits in two
  sentences (see `template.rs` module docs) and is covered by four unit
  tests plus two adapter-level integration tests
  (`crates/omni-web-adapter/tests/compile.rs`) that specifically assert
  user-controlled input cannot produce unescaped `<`/`>`/`&`/quotes in the
  output.
- This is also an honest scope limit: OmniForge does not claim to be a UI
  framework. A real one (components with logic, state, event handlers)
  needs a different adapter with a different, larger security review —
  tracked in `docs/roadmap.md` as Planned, not something this adapter
  quietly grows into.
