# ADR 0014: Honest SEO foundations, scoped to `web-static`'s real single-page output

## Status
Accepted.

## Context
The assignment driving this milestone asked for a substantial SEO
feature set: metadata, sitemap/robots, structured data, semantic/
technical checks, and an `omni seo check`/`report` command — while
explicitly warning against overclaiming ("no framework can guarantee the
highest Google ranking," "do not claim SEO-friendly output if the actual
content is unavailable to search engines"). `web-static` (the only
adapter that renders HTML) has a deliberately narrow, already-documented
scope: one root component, one rendered page, no routing (ADR 0003). Any
SEO feature built here has to honestly inherit that scope rather than
imply multi-page capabilities that don't exist.

## Decision
Add an optional `seo:` block to the web app manifest
(`specs/webapp.schema.json`, `crates/omni-web-adapter/src/manifest.rs::
SeoConfig`) — every field optional, nothing emitted with a guessed value:

- Always-on, zero-config: a responsive viewport meta, a default-
  permissive `robots.txt`, a default `index, follow` robots meta.
- Config-gated: meta description, canonical link (`canonical_url` falling
  back to `site_url`), Open Graph description/image, a minimal `WebSite`
  JSON-LD block, and `sitemap.xml` — the last two specifically require
  `seo.site_url` (an absolute URL), since a sitemap/JSON-LD `url` field
  with a relative or missing URL is not conformant; omitting them when
  unconfigured (with an `I-WEB-SITEMAP-SKIPPED` diagnostic explaining
  why) was chosen over guessing or writing something technically present
  but wrong.
- `omni seo check <target>`/`omni seo report <target>`
  (`crates/omni-cli/src/commands/seo.rs`): real checks against the
  already-built `dist/<target>/index.html` — title, description,
  viewport, canonical, single-`<h1>`, image `alt` text, indexability.
  `check` exits non-zero on any failure (a real CI gate); `report` is the
  same analysis, always exiting 0 (informational).

Two scope decisions were deliberate, not oversights:

- **No schema.org types beyond a minimal `WebSite`.** `FAQPage`,
  `Product`, `LocalBusiness`, etc. all require real data (ratings, prices,
  addresses, dates) this adapter has no source for. Emitting one of these
  without real backing data would be exactly the "misleading structured
  data" this project's own rules forbid — so only `WebSite` (name + url,
  both genuinely known) is implemented.
- **The SEO checker is a string-based heuristic, not an HTML parser.**
  Adding an HTML-parsing dependency for a handful of presence checks
  against output this project's own adapter controls the shape of was
  judged not worth the dependency weight. Documented plainly in the
  command's own module doc and `docs/seo.md`, including the concrete way
  it could be fooled (a literal `>` inside a quoted attribute value).

## Consequences
- Every SEO claim in `docs/seo.md` is scoped to exactly what
  `web-static` can honestly produce: one page, one sitemap entry, no
  SSR/SSG for any other adapter (none render HTML at all). No ranking
  claim is made anywhere.
- `examples/hello-web`'s `web/app.yaml` now carries a real `seo:` block
  and passes `omni seo check web` for real — exercised by
  `scripts/verify.sh`, not just unit tests.
- Adding the `seo` field to the webapp schema is backward compatible:
  it's optional, and every existing example/test without it continues to
  build identically except for the three now-always-on additions
  (viewport meta, default robots meta, `robots.txt`) — verified by the
  full existing test suite passing unchanged.
