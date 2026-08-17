---
version: 1
slug: "templates-base-html"
primary_target: "templates/base.html"
related_targets: ["static/app.css"]
---

# Shared shell — `templates/base.html`

**Scope:** the document shell every surface extends, plus error rendering, the 404, and the icon set. Not a
visitor-facing surface of its own; it is the system all three surfaces inherit.

## What this file owns

- The head: title, OG tags, stylesheet link, and the **icon set that has never existed** — favicon,
  apple-touch-icon, web manifest. A `.fallback(not_found)` catch-all *is* registered (`src/lib.rs:320`) and
  returns `AppError::NotFound` as JSON, so the stale TODO comment in this template is wrong. What is genuinely
  missing is the icon assets and an **HTML 404 for browser navigation** rather than a JSON body.
- The theme mechanism. `<meta name="color-scheme" content="light dark">` is already declared, so both themes
  are expected. The removed toggle lived only on the dashboard nav, which meant the landing, preview, and
  error pages could never change theme — that gap closes here, in the shell, where every surface can reach it.
- The footer credit, if it returns: `made by @seqre.dev`. A design decision, not a fixed requirement.

## Direction

The Gridsmith — Wiener Werkstätte pierced lattice. The shell owns the parts of the system every surface
shares:

- **The frame.** Every block sits inside a ruled frame; borders are runs of small black squares; dividers
  carry square joints; section labels are inverted black tabs.
- **The four-square mark**, appearing on controls as the world's recurring glyph.
- **Silver as a real material** — hammered gradient and texture, not a flat gray fill.
- **Integer-only scaling:** the lattice re-counts its squares per viewport and never stretches a cell.
- **One motion:** the strike. A cell filling solid black. Used everywhere, orchestrated, rather than
  scattered hover effects.

Error states are part of this world, not an escape from it. States print themselves in the system's own
voice — no toasts, no foreign modal chrome.

## The logo, which this world accommodates exactly

`static/logo.svg` is binding and governed by `.claude/skills/logo-guidelines`: monochrome only, pure `#000`
or `#FFF` inside its box, never recolored to an accent, never distorted, never given effects, clear space
≥ the width of the "a" counter, never on a busy field. A black-and-white lattice world gives it a clean
monochrome field natively — the mark sits *on* this palette without adopting it.

**Flagged substitution:** the wordmark should be Now Bold, and no font file exists anywhere in this
repository. It ships on a fallback until the file is supplied.

## Constraints

- Fonts must be **vendored** into `static/` as woff2; rust-embed bakes `static/` into the binary and there is
  no bundler. `style-src 'self'` and `font-src 'self'` stay strict.
- CSP relaxation is authorized for `script-src 'unsafe-eval'` (standard Alpine) and `connect-src` for the
  jetstream hosts, `plc.directory`, and `public.api.bsky.app`. See PRODUCT.md's Stack section.
- `static/app.css` must not be deleted — `base.html` links it and `src/api/static_files.rs` tests assert it
  returns 200 + `text/css`.
- Static assets are served with `Cache-Control: max-age` from `static_cache_max_age` (default 15s). Raise it
  once assets are fingerprinted or stable.

## Unresolved

- The dark rendition. Inverted lacquer ground is the obvious read, but it is a real decision, not a default —
  gallery white and hammered silver do not simply invert.
- Whether the theme toggle is a persistent shell control or lives in a nav that only some surfaces render.
