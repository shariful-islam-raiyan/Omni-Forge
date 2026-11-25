# ADR 0001: Four crates, not twelve

## Status
Accepted.

## Context
The original architecture sketch proposed a dozen crates: `omni-cli`,
`omni-core`, `omni-config`, `omni-registry`, `omni-resolver`,
`omni-contract`, `omni-adapter-api`, `omni-build`, `omni-cache`,
`omni-security`, `omni-diagnostics`, `omni-test`. With exactly one adapter
and one target implemented, most of those would be a few dozen lines
re-exporting types from one another — compile-time overhead and navigation
overhead with no corresponding benefit yet.

## Decision
Ship four crates: `omni-core` (config/schema/components/resolver/graph/
cache/diagnostics/paths), `omni-adapter-api` (the `Adapter` trait +
protocol types + conformance checks), `omni-web-adapter` (the one adapter),
`omni-cli` (binary + orchestrator). Inside `omni-core`, keep the same
module boundaries the twelve-crate design implied (`loader.rs`,
`components.rs`, `resolver.rs`, `graph.rs`, `cache.rs`, `paths.rs`,
`diagnostics.rs`, `schema.rs`) so that splitting any one of them into its
own crate later is a mechanical `mod` → crate move, not a redesign.

## Consequences
- Faster builds and less ceremony today.
- When a second adapter needs to depend on, say, caching without pulling
  in the whole resolver, `omni-cache` can be extracted from `omni-core`
  without touching its public API shape.
- Risk: if `omni-core` grows organically without re-examining these
  boundaries, it could become a dumping ground. Mitigation: the module
  list above is the intended seam; new unrelated concerns should get their
  own module (or crate) rather than being bolted onto an existing one.
