---
version: 1
slug: "templates-home-html"
primary_target: "templates/home.html"
related_targets: []
---

# Landing — `GET /`

**Scope:** the signed-out landing page and its login form. Visitor mode: **Persuade**, demonstration-led.

## Audience and job

An everyday Bluesky or atproto account holder who has never heard of a PDS, arriving from a shared link or
word of mouth. Their job on this page is to understand what this is and sign in. A second visitor also lands
here: someone who followed an atpr.to link, was redirected, and came back to see what the domain was.

## Action

One action: sign in. `POST /api/login` is a **browser form**, not JSON — one field named `handle`, 303 to the
user's PDS authorize URL. That contract is fixed (`Form<LoginRequest>`, `src/auth.rs`).

The hardest moment on this page is typing `alice.bsky.social` exactly right. Handle autocomplete
(debounced `app.bsky.actor.searchActors`, handle + display name + avatar) exists to turn recall into
recognition, and it is the only place real people appear before sign-in.

## Proof — show, don't argue

The proof is the **live Jetstream feed**: real `to.atpr.link` records being written across the network right
now, each carrying a real handle. This replaces every claim the page might otherwise make. No testimonials,
no counts, no customer logos — PRODUCT.md forbids inventing them and the feed makes them unnecessary.

Rebuild it properly this time. The removed version had no reconnect and no `onerror`/`onclose` handling.
It needs backoff, a cap on rendered items, and a designed **quiet state**: this is a low-traffic product and
an empty wall must read as calm, not broken.

Cut from the old page and not returning: the rotating placeholder (motion that teaches nothing while you
type into it) and the long→short fade demo loop (a decoration standing where a real demonstration now is).

## Direction and memorable moment

The Gridsmith — Wiener Werkstätte pierced lattice. The landing's memorable moment is **the strike**: a cell
filling solid black as a record lands in the wall, the same motion the dashboard uses when you create a link.
Your entries render filled; the network's render pierced and open.

The first viewport is a thesis, not a header: the lattice owns the frame at real scale, and sign-in lives
inside it as a ruled cartouche rather than as a form in a card.

## Constraints

- Redirects to `/dashboard` when the visitor has a session that actually restores (not merely a cookie).
- Rate limited — `/api/login` sits behind `GovernorLayer`. A real person can hit it; say so in human words.
- Needs CSP `connect-src` for `public.api.bsky.app` (autocomplete) and the two jetstream hosts (feed).

## Unresolved

- Whether the feed or the sign-in cartouche leads the first viewport at mobile widths.
- Copy for the quiet-feed state.
