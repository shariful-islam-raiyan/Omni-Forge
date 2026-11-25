# ADR 0006: API contract format — a documented OpenAPI 3.1 subset, not a new format

## Status
Accepted. **Superseded in part by [ADR 0008](0008-api-contract-is-a-custom-format.md):**
this ADR's "subset of OpenAPI 3.1" framing turned out to be inaccurate (the
format is not a structural subset of OpenAPI 3.1 by any real OpenAPI
tool's reading — see ADR 0008 for exactly why) and has been corrected
there. The rest of this ADR — the format staying small, what's explicitly
excluded, why a bespoke format vs. a full OpenAPI model were the real
alternatives considered — is still accurate and kept verbatim below as the
historical record of that decision.

## Context
The full-stack milestone needs a versioned, language-neutral way to
describe an HTTP backend's methods, paths, request/response schemas, and
error responses — something both the Python backend's real behavior and a
Rust integration test can be checked against, so "the backend returns
whatever it happens to return" and "the test expects whatever it happens to
expect" cannot silently drift apart. Before writing one line of this,
`docs/roadmap.md` already named this gap ("Route / API contract specs |
Planned | No routing or API-contract feature exists to specify yet").

Three options existed: invent a bespoke OmniForge-specific format; pull in
a full OpenAPI 3.1 document model (`$ref` resolution, `components`,
`servers`, parameters, security schemes, the works); or define a small,
explicitly bounded subset of OpenAPI 3.1 and say so.

## Decision
A documented subset of OpenAPI 3.1 (`specs/api-contract.schema.json`,
`omniforge.dev/api-contract/v1`, JSON Schema draft-07 — the same dialect
every other file in `specs/` already uses):

- `info.title`/`info.version` (OpenAPI's `info` object, unchanged).
- `paths.<path>.<get|post>` — only the two HTTP methods this milestone's
  example actually needs. Adding more later is additive, not breaking.
- `request.body` / `responses.<status>.body` — each an **inline** JSON
  Schema draft-07 fragment, validated with the `jsonschema` crate (already
  a workspace dependency via `omni-core`) exactly the way every other
  OmniForge document is validated (`omni_core::schema`).
- **Explicitly excluded**: `$ref`, `components`, `servers`, parameters,
  security schemes, path templating (`{id}`), any method beyond `get`/
  `post`. None of these have a real consumer yet; adding them speculatively
  would make "subset" a polite fiction.

`crates/omni-core/src/spec/api_contract.rs` parses and schema-validates the
document (mirroring `spec/build_report.rs`'s style) and exposes
`ApiContract::operation(path, method)` for callers — today, only
`crates/omni-cli/tests/fullstack_http.rs`, which loads
`examples/hello-fullstack/backend/api-contract.json` and validates real HTTP
response bodies against its declared schemas.

## Consequences
- No new parsing dependency: the same `jsonschema` crate already compiled
  into `omni-core` for every other spec handles the inline body fragments.
- The format is honestly small. A consumer that needs `$ref`,
  parameters, or path templates will find they are not supported — that is
  the intended boundary, not an oversight, and is written down here rather
  than discovered by surprise.
- `omni-core` validates the contract *document's own shape*; it does not
  (and should not) validate that a running backend actually implements
  every declared operation — that is what `fullstack_http.rs`'s real HTTP
  requests are for. The split mirrors the project's existing pattern of
  "schema says what's allowed; a real adapter run proves what actually
  happens" (ADR 0005).
- An undeclared route (anything not listed under `paths`) has no contract
  entry to validate against by construction — a 404 for such a route is
  checked directly against a well-formed-error-shape expectation in the
  test, not against the contract. See `docs/api-contract-format.md`.
