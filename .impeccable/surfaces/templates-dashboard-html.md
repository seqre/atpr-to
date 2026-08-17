---
version: 1
slug: "templates-dashboard-html"
primary_target: "templates/dashboard.html"
related_targets: []
---

# Dashboard — `GET /dashboard`

**Scope:** the authenticated link manager. Visitor mode: **Operate**. Expression may never obscure the task,
the state, or a familiar affordance.

## Audience and job

The same everyday poster, now signed in and usually mid-composition: a long URL on the clipboard, wanting a
short one back fast, then leaving. Managing links is the rarer, second job.

Ranked by frequency, not by feature count:

1. **Create and copy.** Speed from long URL to copied short URL beats everything else on this screen.
   Creating a link should leave it already in the clipboard, visibly.
2. **Find and verify.** Scan the list, confirm where something points.
3. **Repair.** Edit a destination, delete a link.

## Backend contracts

- `GET /api/links` → `{"links":[{"code","url","updated_at"}],"cursor":…}`, `?limit=&cursor=`
- `POST /api/shorten` → `{url, code?}` → `{short_url}`; **409 when the code is taken**; rate limited
- `DELETE /api/shorten/{code}` → 204
- `POST /api/logout` → form POST, 303 to `/`, clears the cookie
- `GET /@{handle}/{code}/qr` → standalone `image/svg+xml`

Every error body is `{"error": "..."}` with a JSON content type on every route, so one shared handler covers
all of them. The old `alert()` handling is gone and must not return.

`handle` and `avatar` come from `DashboardTemplate` (`src/api/ui.rs`); `avatar` is an empty string when the
profile lookup returns nothing. Both must stay referenced in the markup or `clippy -D warnings` fails the
build.

## Feature decisions

**Promoted:** copy-to-clipboard (terminal action of the primary job); inline destination edit — the one place
"you own this record" becomes tangible, so it is a first-class control, never a hover-reveal; delete with a
confirmation that names the code rather than asking "Are you sure?".

**Kept:** text filter on the list. QR dialog with SVG download, as a secondary control that never competes
with copy.

**Demoted:** sort headers. Realistic range is 1–20 links for almost everyone; at that size sort is ceremony.
Newest-first by default; sort appears only past a threshold.

**Cut:** the local/UTC timestamp toggle — a control for a problem the design should not have. Show relative
time with the absolute in `<time datetime>`. Note `updated_at` arrives as a raw ISO-8601 string and there is
no client formatter in the build any more.

**Added:** the **empty state** — a first-time user with zero links, which is the activation moment and had no
design before. A blank lattice with the first cell waiting.

## Direction and memorable moment

The Gridsmith. The memorable moment is **the strike**: creating a link fills a cell solid black and the
maker's mark sets — the moment the record is written to your own repo, made material.

The hard discipline here: the display voice is not the table voice. Letterspaced capitals are hostile at
small sizes, and this screen is where the direction is won or lost. Dense rows get a quiet register inside
the same grammar.

## Constraints

- `/api/links` paginates by cursor. "More" must be honest rather than implying the list is complete.
- Destinations run to 2048 characters and must truncate without lying about where they point.
- 409 conflict renders inline on the create form, in the world's own vocabulary.
- Rate limits are reachable by a real person; give them human words.
- The **known `/tmp` session concurrency bug** makes logins fail intermittently. It is user-visible and
  currently generic — it needs an honest state that tells someone to try again.

## Unresolved

- The threshold at which sort controls appear.
- Whether the QR dialog and the edit affordance share one secondary control cluster.
