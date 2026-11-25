# ADR 0008: the API contract format is a custom OmniForge format, not an OpenAPI 3.1 subset

## Status
Accepted. Supersedes the compatibility claim in
[ADR 0006](0006-api-contract-format.md) (ADR 0006's scoping rationale —
why the format stays small, what's excluded — is still accurate; only its
"subset of OpenAPI 3.1" framing was wrong).

## Context
As part of the "Unified Application Runtime and Developer Experience"
milestone, the task was to "inspect the existing OpenAPI 3.1 subset and its
use of JSON Schema draft-07, resolve the compatibility issue accurately."
Doing that inspection found the compatibility claim itself was the problem,
not just the draft-07/2020-12 dialect mismatch it named.

`specs/api-contract.schema.json`'s own title, `docs/api-contract-format.md`,
`docs/roadmap.md`, `CHANGELOG.md`, and `crates/omni-core/src/spec/api_contract.rs`'s
module doc all described this format as "a documented subset of OpenAPI
3.1." A real OpenAPI 3.1 document:

- Has a required top-level `openapi: "3.1.x"` field. This format has
  `schema: "omniforge.dev/api-contract/v1"` instead — a different field
  with a different value space, not an OpenAPI version string.
- Expresses a request/response body as `requestBody.content.<media-type>.schema`
  / `responses.<code>.content.<media-type>.schema` — a media-type-keyed
  map, because OpenAPI supports more than one representation per
  status/request. This format has a flat `request.body` /
  `responses.<code>.body` — no `content`, no media type key at all.
- Declares its Schema Object dialect as
  `https://spec.openapis.org/oas/3.1/dialect/base`, built on JSON Schema
  **2020-12**. This format's body fragments (and its own document schema)
  are JSON Schema **draft-07** (`specs/api-contract.schema.json`'s own
  `$schema`), an earlier, structurally different dialect (e.g.
  `exclusiveMinimum`/`exclusiveMaximum` are booleans attached to
  `minimum`/`maximum` in draft-07, plain numeric keywords in 2020-12/OAS
  3.1; `prefixItems` doesn't exist in draft-07).

Any one of these would already make a document in this format fail to
parse as OpenAPI 3.1 in a real OpenAPI toolchain (missing required
`openapi` field alone is disqualifying). So "a subset of OpenAPI 3.1" was
not an approximate truth that happened to use an older schema dialect —
it described a document shape that was never a structural subset of
OpenAPI 3.1 to begin with. The three options from ADR 0006 (bespoke format,
full OpenAPI 3.1 model, or a true bounded subset) were revisited: a real
OpenAPI 3.1 document model would mean `openapi`/`content`/media-type
negotiation/the 2020-12-based dialect/`$ref` resolution/`components` — a
large addition with no current consumer (this project has exactly one
contract-reading test suite, `fullstack_http.rs`, and it needs none of
that). That is disproportionate to this milestone's actual need: the
compatibility *claim* was the defect, not the format's size.

## Decision
Keep the format exactly as it is structurally (see ADR 0006 for what's in
scope: `info`, `paths.<path>.<get|post>`, `request.body`,
`responses.<code>.{description,body}`, draft-07 inline body fragments).
Stop calling it OpenAPI. Rename/redocument it everywhere as what it
actually is: **a small, custom, OmniForge-specific contract format that
borrows OpenAPI's vocabulary (paths, operations, responses) for
familiarity, but is not an OpenAPI document and is not readable by OpenAPI
tooling.**

Concretely:
- `specs/api-contract.schema.json`'s `title` no longer says "subset of
  OpenAPI 3.1."
- `crates/omni-core/src/spec/api_contract.rs`'s module doc, `docs/api-contract-format.md`,
  `docs/architecture.md`, and `docs/roadmap.md` all describe this as a
  custom format and link here.
- `ADR 0006` is kept verbatim (historical record of the original
  decision) with one line added noting it is superseded by this ADR for
  the compatibility claim specifically.
- Added real request/response body validation as a reusable library API
  (`ApiOperation::validate_request_body`, `ApiResponse::validate_body` in
  `crates/omni-core/src/spec/api_contract.rs`) instead of the ad hoc
  `jsonschema` calls that previously lived only in
  `crates/omni-cli/tests/fullstack_http.rs` — this was true regardless of
  the naming question, but doing it here keeps "what validates a body" and
  "what that body format actually is" documented in the same place.

## Consequences
- No functional/wire-format change: every existing `api-contract.json`
  document (including `examples/hello-fullstack/backend/api-contract.json`)
  continues to parse and validate identically. This is a documentation and
  naming correction plus a library API addition, not a breaking change.
- A future "real OpenAPI 3.1 support" would be a genuinely new, larger
  feature (a document model with `$ref` resolution, `components`, a
  2020-12-based dialect, media-type-aware bodies) — tracked as Planned in
  `docs/roadmap.md`, not implied by anything shipped today.
- Anyone who had generated tooling against this format expecting OpenAPI
  compatibility (nothing in this repository did) would have already hit
  the structural mismatches above; this ADR documents why, rather than
  leaving it to be independently rediscovered.
