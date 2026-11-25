# API contract: `api-contract.json`

Schema: `specs/api-contract.schema.json` (`schema:
omniforge.dev/api-contract/v1`). Loader/validator:
`crates/omni-core/src/spec/api_contract.rs`. Rationale for the format
choice: [decisions/0006-api-contract-format.md](decisions/0006-api-contract-format.md).

## What this is

A **small, custom OmniForge format** — not OpenAPI, and not claimed to be
(see [ADR 0008](decisions/0008-api-contract-is-a-custom-format.md) for why
an earlier "subset of OpenAPI 3.1" description here was inaccurate and got
corrected). It borrows familiar OpenAPI vocabulary — `paths`, an
HTTP-method-keyed operation, `responses` — for readability, but is not a
valid OpenAPI document: there's no `openapi` version field, request/response
bodies are a flat `body` key rather than `content.<mediaType>.schema`, and
body fragments are plain JSON Schema draft-07 (the same dialect every other
`specs/*.schema.json` file in this repository already uses), not OpenAPI
3.1's 2020-12-based dialect. See "Explicitly out of scope" below for what
else isn't modeled.

## Shape

```json
{
  "schema": "omniforge.dev/api-contract/v1",
  "info": { "title": "hello-fullstack backend API", "version": "1.0.0" },
  "paths": {
    "/api/greeting": {
      "get": {
        "summary": "Return the configured greeting.",
        "responses": {
          "200": {
            "description": "A greeting was produced.",
            "body": {
              "type": "object",
              "required": ["greeting", "count"],
              "properties": {
                "greeting": { "type": "string" },
                "count": { "type": "integer" }
              }
            }
          }
        }
      }
    },
    "/api/echo": {
      "post": {
        "summary": "Echo a message back as JSON.",
        "request": {
          "body": {
            "type": "object",
            "required": ["message"],
            "properties": { "message": { "type": "string" } }
          }
        },
        "responses": {
          "200": { "description": "Echoed.", "body": { "type": "object" } },
          "400": { "description": "Invalid request body.", "body": { "type": "object" } }
        }
      }
    }
  }
}
```

The full worked example lives in
`examples/hello-fullstack/backend/api-contract.json`.

## Fields

- `schema` — must be exactly `omniforge.dev/api-contract/v1`.
- `info.title`, `info.version` — free text, required.
- `paths.<path>` — keys must start with `/`. Each path maps method names
  (`get` or `post` only — see scope below) to an operation.
- `paths.<path>.<method>.summary` — optional, free text.
- `paths.<path>.<method>.request.body` — optional. An inline JSON Schema
  draft-07 fragment the request body must match.
- `paths.<path>.<method>.responses.<status code>` — required, at least
  one. Keyed by HTTP status code as a string (`"200"`, `"400"`, `"404"`,
  ...). Each has a required `description` and an optional `body` fragment.

## How it's used

Two layers, both in `crates/omni-core/src/spec/api_contract.rs`:

1. **Document-shape validation.** `omni_core::spec::api_contract::parse`
   schema-validates `api-contract.json` itself (`specs/api-contract.schema.json`)
   and deserializes it into an `ApiContract`. `ApiContract::operation(path, method)`
   looks up the declared operation for a given path/method. If a target
   declares `contract: backend/api-contract.json` in `omni.yaml`
   (see [project-format.md](project-format.md)), `omni build`/`omni check`
   run this validation automatically as part of building that target — an
   invalid contract fails the build, and the contract's bytes are part of
   that target's build cache key (editing the contract busts the cache).
   Before this milestone, this validation only ever ran inside a
   hand-written integration test, never during a real build.
2. **Payload validation.** `ApiOperation::validate_request_body(&body)` and
   `ApiResponse::validate_body(&body)` compile the relevant inline `body`
   fragment with the `jsonschema` crate and check a real JSON value against
   it, returning every violation message. `crates/omni-cli/tests/fullstack_http.rs`
   uses these to assert a real HTTP response (and a real 400 for an invalid
   request) from an actually-running backend process matches what the
   contract declares.

This is the same two-step pattern every other OmniForge document uses:
schema-validate the document's own shape, then separately check it against
reality (here: an actual HTTP request/response, the way ADR 0005 checks a
build's claimed artifacts against the filesystem). `omni-core` validates
the contract document's shape and individual payloads against it; it does
not — and, by design, cannot — verify that a running backend actually
implements every path it declares *without* a real request being made,
which is what `fullstack_http.rs`'s real HTTP requests are for.

## Explicitly out of scope (not a bug if you hit one of these)

- `$ref` and `components` — every body is written inline. A contract that
  repeats the same fragment across several responses repeats it; there is
  no shared-definition mechanism yet.
- `servers` — this format describes paths and methods, not where to reach
  them. The caller decides the host/port (in the shipped example: a
  `--serve <port>` argument to the backend script; see
  [decisions/0007-backend-dev-server-is-example-scoped.md](decisions/0007-backend-dev-server-is-example-scoped.md)).
- Path parameters (`/users/{id}`), query parameters, headers — not
  modeled. Every path in `paths` is matched literally.
- Any HTTP method besides `get`/`post` — rejected by
  `specs/api-contract.schema.json`'s `propertyNames` constraint.
- **Undeclared routes.** A path/method this document does not list is, by
  definition, outside its contract — there is no "wildcard" or "default"
  entry. A server's 404 for such a route is a property of the server, not
  something this format can express or that a test can check "against the
  contract"; `fullstack_http.rs`'s unknown-route test asserts the server's
  own well-formed error shape directly instead.

Widening any of these is a schema version bump
(`omniforge.dev/api-contract/v2`) with its own migration note here, exactly
like every other spec in this repository — see
[project-format.md](project-format.md)'s compatibility policy for the
pattern this would follow.
