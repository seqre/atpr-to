# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack

Server-rendered Rust: Axum + Askama templates (`templates/`), assets baked into the binary by rust-embed
from `static/`. No bundler, no `package.json`, no build step for assets — adding an asset means dropping a
file into `static/` and rebuilding.

**Resolved 2026-08-17:** hand-written CSS (no framework) plus **Alpine.js 3 vendored into `static/`**, serving
the chosen visual world. The previous stack (Pico CSS v2 + Alpine.js 3, both CDN-loaded without SRI) was
removed on the `rebrand` branch; `static/app.css` is empty and every template is bare. Pico is not coming
back — a classless framework's defaults would fight the committed world at every control.

**CSP relaxation is authorized** (user decision, 2026-08-17). The policy in `src/lib.rs` is currently
`default-src 'self'` with no `'unsafe-inline'`, which blocks three things the design needs. Planned changes,
to be applied during implementation:

| Directive | Why |
|---|---|
| `script-src 'self' 'unsafe-eval'` | Standard Alpine 3 evaluates directive expressions with `new Function`. The alternative is Alpine's official CSP build, which forbids inline expressions and requires `Alpine.data()` components — worth weighing before adding `unsafe-eval` |
| `connect-src` for `wss://jetstream1.us-east.bsky.network`, `wss://jetstream2.us-east.bsky.network` | The live network feed |
| `connect-src` for `https://plc.directory`, `https://public.api.bsky.app` | DID resolution for feed entries, and handle autocomplete on the login field |

`style-src 'self'` and `font-src 'self'` stay strict: all CSS lives in `static/app.css` and fonts are
vendored same-origin. `img-src 'self' https: data:` already covers avatars.

No bundler exists and none is being added: fonts and JS are dropped into `static/` and rust-embed bakes them
into the binary.

## Users

**Primary: everyday Bluesky and atproto account holders** who want a short link for something they are about
to post. They know their handle; they do not necessarily know what a PDS, a DID, or a lexicon record is, and
should not have to.

This has a direct consequence: **the product must explain itself.** The core promise — that the link is
stored in the user's own repo rather than in someone else's database — is the reason to choose atpr.to over
any other shortener, and it has to land for someone who has never heard the word "PDS."

A second, unavoidable audience arrives without signing in: **anyone who follows or previews a short link.**
They meet the product at `/@{handle}/{code}` or its `/info` preview page, usually with no context at all.

## Product Purpose

atpr.to turns a long URL into `https://atpr.to/@{handle}/{code}`. The short link is written to the user's own
Personal Data Server as a `to.atpr.link` record via `com.atproto.repo.putRecord`; atpr.to keeps no central
link database. Sign-in is AT Protocol OAuth.

Success is a user creating a short link, sharing it, and it resolving reliably — while understanding that the
link is *theirs*, portable with their account, and not hostage to this service continuing to exist.

## Positioning

**Your short links live in your repo, not our database.** Every competing shortener's links die when the
service does, or change destination when the service decides they should. Because an atpr.to link is a record
in the user's own PDS, ownership is structural rather than promised: the user can read, edit, or delete the
record with any atproto client, and it travels with them if they migrate hosts.

Supporting facts a neighboring product could not copy without adopting the same architecture:
- Creation uses `swapRecord: null`, so a duplicate code is a **409 Conflict** from the PDS — never a silent
  overwrite of someone's existing link.
- Resolution is public and needs no account: Slingshot first (2 hops), falling back to direct PDS resolution
  (3 hops).
- The destination's scheme is validated on read as well as on write, so a record written by some *other*
  client cannot smuggle a `javascript:` destination into a redirect or a preview page.

## Operating Context

- **Create** happens on the dashboard, usually mid-composition: the user has a long URL on the clipboard and
  wants a short one back fast, then returns to whatever they were posting.
- **Manage** happens rarely and in bulk-ish glances: find a link among others, check where it points, fix a
  destination, delete something stale.
- **Follow** happens off-site entirely — someone taps a shared link and is redirected. The `/info` preview
  page (destination, last-modified, QR code) is the only surface they may ever see.
- Sign-in is an OAuth round trip through the user's own PDS, so the user leaves the site and returns.
- Deployed as a single AWS Lambda function behind API Gateway; every page is a cold-start candidate and every
  resolution is a network hop to third-party infrastructure. Latency and failure are normal operating
  conditions, not edge cases.

## Capabilities and Constraints

Confirmed, working today (backend complete; only the UI is stripped):

| Surface | Route | Notes |
|---|---|---|
| Landing / login | `GET /` | `POST /api/login` is a **browser form** target (`Form<LoginRequest>`), one field named `handle`; 303s to the PDS authorize URL |
| Dashboard | `GET /dashboard` | auth-gated; has `handle` and `avatar` (avatar may be empty) |
| List links | `GET /api/links` | `{"links":[{"code","url","updated_at"}],"cursor":…}`, `?limit=&cursor=` |
| Create | `POST /api/shorten` | `{url, code?}` → `{short_url}`; **409** when the code is taken; rate limited |
| Repoint | `PUT /api/shorten/{code}` | `{url}` → 204; replaces the destination, keeps the code; rate limited |
| Delete | `DELETE /api/shorten/{code}` | 204 |
| Log out | `POST /api/logout` | form POST, 303 to `/`, clears cookie |
| Resolve | `GET /@{handle}/{code}` | public redirect |
| Preview | `GET /@{handle}/{code}/info` | destination, `updated_at` (raw ISO-8601), server-rendered QR |
| QR | `GET /@{handle}/{code}/qr` | standalone `image/svg+xml` |

Constraints:
- Destinations must be `http`/`https`, max 2048 characters.
- Every error response on every route is JSON: `{"error": "..."}`. Bodies are opaque for upstream and
  internal failures. One shared client-side error handler is sufficient — the old `alert()` handling is gone
  and must not return.
- Rate limiting applies to `/api/login`, `/api/logout`, `/api/shorten`, `/api/shorten/{code}`, `/api/links`,
  and `/oauth/callback`. A user *can* hit it; the UI has to say so in human terms.
- `updated_at` reaches the template as a raw ISO-8601 string; there is no client-side JS runtime doing
  `toLocaleString()` any more.
- Template fields must stay referenced in the markup — `missing_docs` and `clippy -D warnings` make an unused
  field a build failure, not a warning.
- **Known defect, not yet fixed:** OAuth sessions live on the Lambda instance's `/tmp`, so logins fail
  intermittently under concurrency. The failure is user-visible and needs an honest error state.
- No favicon, apple-touch-icon, or web manifest has ever existed. A catch-all `.fallback(not_found)` *is*
  registered (`src/lib.rs:320`), so unmatched paths return `AppError::NotFound` as JSON — the contrary claim
  in `templates/base.html`'s TODO comment is stale. What is genuinely missing: the icon assets themselves,
  and an HTML 404 for browser navigation rather than a JSON body.

Terminology: **short code**, **destination**, **handle**, **short link**. Prefer these over "slug", "target",
"username". PDS/DID/repo vocabulary is accurate but is *not* the primary user's vocabulary — use it only where
it earns its place.

## Brand Commitments

- **Name:** atpr.to, always lowercase.
- **Logo:** `static/logo.svg` is the current, binding mark. `.claude/skills/logo-guidelines` governs it and is
  authoritative: monochrome only (pure `#000` or `#FFF` inside its box), ink-on-light or paper-on-dark,
  never recolored to an accent, never distorted, never given effects, clear space ≥ the width of the "a"
  counter on all sides, never on a busy or photographic field.
- **Wordmark type:** Now Bold preferred; Inter Bold, then Helvetica Bold, then system sans-serif bold as
  fallbacks — a fallback must be flagged in handoff notes. This governs the wordmark lockup only.
  **Flagged, per that rule:** no font files exist anywhere in this repository (`static/` holds only
  `app.css` and `logo.svg`), so Now Bold is **not currently available** and the wordmark will ship on a
  fallback until the file is supplied. This is a substitution, not a choice.
- **Open, not binding:** the same skill describes a site palette (tinted warm neutrals, one restrained
  accent) and headline type, attributing them to a `DESIGN.md` that **does not exist in this repository**. The
  user has confirmed that prose is a leftover pointer, not a commitment. That prose is now superseded: the
  visual world was chosen on 2026-08-17 (see the surface briefs under `.impeccable/`), and DESIGN.md will be
  written from the built result at finish. Only the mark and wordmark rules above constrain it.
- Author/maintainer: seqre (`@seqre.dev`). The removed footer credited them; whether that returns is a design
  decision, not a fixed requirement.

## Evidence on Hand

- `static/logo.svg` — the official mark, in ink and paper variants.
- A complete, tested, deployed backend: all routes above work; `README.md` and `CLAUDE.md` accurately describe
  them; test suite is hermetic with an 80% line coverage gate.
- `templates/*.html` and `static/app.css` carry detailed handoff comments listing exactly what was removed and
  every backend contract the rebuilt UI must honor. Read them before building.
- Real dependencies worth crediting: [Jacquard](https://github.com/fatfingers23/jacquard) (atproto OAuth/XRPC)
  and [Microcosm](https://microcosm.blue)'s [Slingshot](https://github.com/microcosm-blue/slingshot).

**Absent — must not be fabricated:** no users, no usage numbers, no testimonials, no press, no case studies,
no customer logos, no uptime figures, no pricing or licensing tiers, no roadmap promises, no team page. The
product is free and unmonetized; do not invent a business around it.

## Product Principles

1. **Ownership is the story.** Every surface should make it obvious, to someone who has never heard of a PDS,
   that this link belongs to them and outlives the service.
2. **The destination is the point.** Creating a short link is a means to posting something. Speed from long
   URL to copied short URL beats every other interaction on the dashboard.
3. **Strangers arrive first.** The resolve and preview paths reach far more people than the dashboard ever
   will, and they arrive with no context and no account.
4. **Say what actually happened.** Rate limits, 409 conflicts, and the concurrency login bug are real, visible
   states. Name them plainly instead of hiding behind a generic failure.
5. **Nothing is claimed that isn't true.** No adoption numbers, no social proof, no invented endorsements —
   the architecture is the argument.

## Accessibility & Inclusion

No product-specific standard has been established by the user. Two facts constrain any future work: the base
template declares `<meta name="color-scheme" content="light dark">` (both themes are expected), and the
previous theme toggle only existed on the dashboard, so the landing, preview, and error pages could never
change theme — a gap worth closing rather than reproducing.
