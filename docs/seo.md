# SEO

Status definitions match the rest of this project's docs: **Implemented**
has passing tests and works today; **Planned** is designed/discussed,
nothing built.

OmniForge does not and cannot guarantee search ranking. Ranking depends
on content quality, competition, site authority, and search engines' own
decisions — none of which a build tool controls. What's below is a real,
honest technical-SEO foundation: the things a static site *can* get right
by default, verified by tests, with no feature claiming more than it does.

## Scope: `web-static` only, one page

Only the `web-static` adapter produces HTML, and it renders exactly one
page per target (see [ADR 0003](decisions/0003-template-only-rendering.md)
and `docs/architecture.md`) — no routing, no multiple pages. Every SEO
feature below inherits that scope honestly:

- `sitemap.xml` lists exactly one URL.
- There is no duplicate-title/duplicate-description check, because there
  is only ever one page to compare against itself.
- No SSR/SSG/pre-rendering claim is made for any other adapter — none of
  them render HTML at all today.

## What's implemented

All of the following come from an optional `seo:` block in a target's
`web/app.yaml` (schema `omniforge.dev/webapp/v1`, see
`specs/webapp.schema.json`). Every field is optional; nothing below is
emitted with a guessed or fabricated value — see "What's honestly skipped
without configuration" below.

```yaml
schema: omniforge.dev/webapp/v1
title: "Example"
root:
  component: ui.home
  inputs: { ... }
seo:
  description: "A real, human-written description."
  site_url: "https://example.com"      # required for sitemap.xml + default canonical
  canonical_url: "https://example.com/some-other-canonical"  # overrides site_url for canonical only
  og_image: "https://example.com/og.png"
  robots: "index, follow"              # default if omitted
  lang: "en"                           # default if omitted
```

| Feature | Status | Produced by |
|---|---|---|
| `<meta name="viewport">` | Implemented | Always emitted, no config needed — `crates/omni-web-adapter/src/lib.rs::build_seo_head` |
| `<meta name="description">` | Implemented | From `seo.description`; omitted (not emitted empty) if unset |
| `<meta name="robots">` | Implemented | From `seo.robots`, default `"index, follow"` |
| `<link rel="canonical">` | Implemented | From `seo.canonical_url`, falling back to `seo.site_url`; omitted if neither is set |
| Open Graph (`og:type`/`og:title`/`og:description`/`og:image`) | Implemented | `og:title` always present (from the page's own `title`); description/image only if configured |
| `<html lang="...">` | Implemented | From `seo.lang`, default `"en"` |
| `robots.txt` | Implemented | Always generated: `Allow: /` for everyone, plus a `Sitemap:` line only if `seo.site_url` is set |
| `sitemap.xml` | Implemented | Only if `seo.site_url` is set (needs an absolute URL to be conformant) — honestly skipped otherwise, with an `I-WEB-SITEMAP-SKIPPED` diagnostic explaining why, never written with a relative or placeholder URL |
| JSON-LD structured data (`WebSite`) | Implemented | Only if `seo.site_url` is set; `name` + `url` only — this adapter has no author/organization/publish-date data to honestly populate anything richer |
| `omni seo check <target>` | Implemented | Real checks against the already-built `dist/<target>/index.html` — see below |
| `omni seo report <target>` | Implemented | Same checks, always exits 0 (informational, not a gate) |

## What's honestly skipped without configuration

No `seo:` block at all still gets the always-on defaults (viewport,
default-permissive robots meta, `robots.txt`) but **not**: a description
meta, a canonical link, Open Graph description/image, JSON-LD, or
`sitemap.xml`. Nothing is filled in with a guess — see
`crates/omni-web-adapter/tests/compile.rs::
without_site_url_sitemap_is_honestly_skipped_but_robots_still_allows_all`.

## `omni seo check`/`omni seo report`

```sh
omni build web          # build first -- these commands read dist/<target>/index.html
omni seo check web      # exits non-zero if any check fails; usable as a CI gate
omni seo report web     # same checks, always exits 0
omni seo check web --json
```

Checks performed (`crates/omni-cli/src/commands/seo.rs`): non-empty
`<title>`, a meta description present, a viewport meta present, a
canonical link present, exactly one `<h1>`, every `<img>` apparently has
`alt=`, and the page isn't marked `noindex`.

**Implementation honesty**: this is a lightweight **string-based
heuristic scan**, not a real HTML/DOM parser — this project does not take
on an HTML-parsing dependency for it. It's accurate for the output
`web-static` itself produces, but unusual hand-written markup inside a
component's own template (e.g. a literal `>` character inside a quoted
attribute value) could confuse it. This is stated in the command's own
module doc, not discovered the hard way by a user.

## What's explicitly NOT implemented

- **Server-side rendering / static site generation for any other
  adapter.** None exists — `web-static` is the only adapter that renders
  HTML, and it has no SSR/SSG mode beyond its existing one-shot static
  render.
- **Multi-page sitemaps, duplicate title/description detection across
  pages, internal/broken link checking.** All of these need more than
  one page to mean anything; `web-static` only ever produces one.
- **Lighthouse or Core Web Vitals integration.** Not wired up. A real
  project should run Lighthouse itself against the built output.
- **Image optimization, responsive `srcset` generation, lazy loading,
  font-loading strategy.** `web-static` copies declared assets verbatim
  (see `docs/architecture.md`) — none of this is implemented.
- **FAQPage, Product, LocalBusiness, Event, or any schema.org type other
  than a minimal `WebSite`.** Adding one of these honestly requires data
  (ratings, prices, addresses, dates) this adapter has no source for;
  fabricating it would violate this project's "no misleading structured
  data" rule, so it isn't done.
- **Internationalization (`hreflang` alternates).** Not implemented —
  `seo.lang` sets one language for the one page; there is no concept of
  per-locale alternates since there's only one page.
