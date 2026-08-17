---
version: 1
slug: "templates-info-html"
primary_target: "templates/info.html"
related_targets: []
---

# Link preview — `GET /@{handle}/{code}/info`

**Scope:** the public preview page for a single short link. Visitor mode: **Read**, with one clear action.

## Audience and job

A total stranger. No account, no session, no context — usually someone who was handed a short link and wants
to know where it goes before following it. This surface and the bare redirect reach far more people than the
dashboard ever will, and they arrive cold.

Their question is exactly three things: **where does this go, whose is it, and is it current.**

## Action

One primary action: follow the link. The removed design had a "Follow this link" button and it was right —
reinstate it as the page's clear primary action, in the world's own vocabulary.

## Content and contract

All five `InfoTemplate` fields (`src/api/info.rs`) must stay referenced in the markup or `clippy -D warnings`
fails the build: `handle`, `code`, `url`, `updated_at`, `qr_svg`.

- `qr_svg` is generated server-side and is the **only `|safe` expression in the codebase**. Keep it `|safe`;
  keep everything else escaped.
- `updated_at` is a raw ISO-8601 string and is `Option` — it may be absent.
- The destination's scheme is validated on read as well as write, so a record written by some other client
  cannot smuggle a `javascript:` URL onto this page. That guarantee is load-bearing for a page whose whole
  job is showing a stranger where a link points.

## Direction and memorable moment

The Gridsmith — and this page is where the world pays off hardest.

**A QR code is a pierced square lattice in black and white.** The Wiener Werkstätte pierced basket and the QR
code are the same object. This page already generates one server-side, which makes it the most
product-specific asset atpr.to owns and it is native to this world at full scale. The QR is not a utility
tucked in a corner here; it is the page's hero material, set large.

Beside it, the short link at **poster scale** — size is billing — with the handle as the maker's mark. The
destination is shown in full and honestly. The visitor should be able to answer all three of their questions
from the first viewport without scrolling.

Provenance is visible: who made this and when, carried on the record rather than implied.

## Constraints

- No session, no personalization, no nav that assumes an account.
- Handles run 8 to 40+ characters and destinations to 2048; both must wrap or truncate without lying about
  where the link points.
- `updated_at` may be absent — the layout cannot depend on it.
- Successful redirects are cached per `redirect_cache_max_age`; errors are always `no-store`.

## Unresolved

- Whether the destination's full URL or its bare domain leads at mobile widths.
- Whether this page offers any onward path to `/` for a visitor who has never heard of atpr.to. It is the
  best acquisition surface the product has, and it currently offers nothing.
