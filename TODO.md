# TODO

Open items, split by who is blocking. 2026-08-18, 27 commits on `rebrand`, none pushed.
This file and `flake.*` stay untracked.

## Yours

Mostly decisions and one file I cannot produce. Each says what I do once it's answered.

1. **Decide what an unresolvable handle returns.** `/@nosuchuser.example/abc` is a 502 today.
   jacquard's `HandleResolutionExhausted` is ambiguous by its own admission — the handle may not
   exist, *or* DNS/HTTPS/PDS is down. I'd map it to 404: a typo is the common case, and a total
   resolution failure means the app is down anyway. Your call because it decides what a stranger
   is told during an outage. → I implement it with a test.
- return human-readable "handle not found" or similar error

2. **Resize the browser window** so the width pass can run. This Chrome connection refuses to do
   it itself — `outerWidth` reports 0 and `resizeTo` is ignored. → I screenshot 320 / 375 / 768 /
   1024 / 1440 in both renditions and report what breaks. Nothing about width has ever been
   observed, so every breakpoint claim in `DESIGN.md` is currently a claim about the stylesheet.

3. **Supply Now Bold**, or accept the Cinzel substitution permanently. Licensed file, not
   something I can obtain. → I swap it in and remove the flags from `PRODUCT.md`, `DESIGN.md` and
   `static/fonts/README.md`.
- done

4. **Install a rasteriser** (`rsvg-convert`, ImageMagick or `cairosvg` in the flake) — there is
   none in this shell. → I cut `apple-touch-icon.png` and a PNG `og:image` from the mark with the
   same clear space as `static/favicon.svg`. Until then shared links unfurl with no image,
   because `base.html` points `og:image` at an SVG.
- provide a command and I'll do that

5. **Choose the dashboard's sort threshold** — at what link count sort controls should appear.
   Newest-first only today, and no number was ever picked. → I build it.
- more than 10

6. **Pick a session backend** to replace `/tmp` (DynamoDB, Redis, S3 — this runs on Lambda, so
   sessions are per execution environment and logins fail intermittently under concurrency).
   → I implement it behind the existing `session/` seam, no handler changes.
- add another one for dynamodb

7. **Say whether to delete the test link** — `gridsmith` on `atprto-test.pds.rip`, pointing at the
   Rust book. I made it to exercise the dashboard and left it rather than deleting from your
   repository. → One click if you want it gone.
- leave it, it's fine

## Mine

No decisions needed; I can start any of these now.

1. **Get `cargo deny check` green.** Pre-existing — `Cargo.lock` is byte-identical to the commit
   this work started from. `RUSTSEC-2026-0258` (`h2`, unbounded empty DATA frames) first: a real
   DoS vector on the stack every outbound call uses. Then `RUSTSEC-2026-0119`,
   `RUSTSEC-2023-0071`, `RUSTSEC-2026-0190`, `RUSTSEC-2023-0089`, plus `webpki-roots 1.0.7`
   (`CDLA-Permissive-2.0`) missing from `deny.toml`'s allowlist. Mostly via jacquard and reqwest;
   likely a bump plus one allowlist entry. I'll say which advisories a bump cannot reach.

2. **Humanise the preview page's timestamp.** It shows `2026-08-18T17:05:20.042538Z` to a
   stranger deciding whether to follow a link. The raw string only has to survive in
   `<time datetime>` for the test that depends on it; the visible text is free. Formatting goes
   in Rust — that page runs no JS.

3. **Install `detect.mjs`'s parser modules** — `htmlparser2`, `css-select`, `css-tree`,
   `domutils`. Without them it falls back to regex and evaluates neither custom properties nor
   computed contrast, so its clean result means nothing. Say if you'd rather I not `npm install`
   into the skill directory.
- do it

## Blocked on the above

- **Revisit the landing page's mobile order** (compressed wall above sign-in). It was a judgement
  call made without seeing it, and it needs Yours #2 before it can be judged.
  - on small width, the sign out button is getting too large (bigger than theme switcher button)
  - on small widths, merge all buttons into one with three dots extending into list of actions and keep them on the same line as link title/url
  - on small width, the spacing in the link table is weird, there is big gap between the white square and link title/url

## Yours I found

- on the sign in page, there is pointless four-square symbol at the end of handle input bar, it does nothing and there is "sign in" button next to it
- the theme switcher button needs a recognizable icon
- the "what actually get written" section need to be rewritten/remade to be more non-technical user friendly
- add the placeholder carousel in the handle input bar with different pds'es
- the font in the buttons needs to be bolder or bigger, it's a bit hard to read
