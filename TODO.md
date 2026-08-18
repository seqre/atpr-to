# TODO

2026-08-18. 44 commits on `rebrand`, none pushed. This file and `flake.*` stay untracked.

Everything from the last round is done, and so are the three "next round" items — the wall now
distinguishes create/repoint/delete and shows only the handle and the verb, the record section
says Atmosphere, and the rendition glyphs are curves rather than pixel art.

## Yours

1. **Confirm the Now Bold licence.** It ships in `static/fonts/` and sets the wordmark. Unlike
   Cinzel and Jost it is not an open licence, and no licence text sits beside it. Self-hosting a
   commercial face in a public deployment is a question only you can answer;
   `static/fonts/README.md` records it as "supplied by the project owner" until you say otherwise.

2. **Deploy, and watch a login.** DynamoDB sessions are implemented, tested against the SDK's
   replay client, and the stack validates — but no real table has ever been written to, because
   that needs AWS credentials. The first deploy is the real test: sign in from two places and
   confirm neither login fails.

## Mine

3. **Touch, and a second browser.** Every measurement so far is a fine pointer in Chrome. The
   `(pointer: coarse)` rules that lift controls to 44px are unexercised, and no Firefox or Safari
   has seen any of this. The dressed `<select>` and the native `<dialog>` are the two most likely
   to differ.

4. **Three test links are now in the test account**, all mine: `alpha-sort` and `zulu-sort` from
   the sort work, and `jet-lr3n` / `live-56z5` from proving the live wall works end to end. Say
   the word and they go; `gridsmith` stays per your earlier call.

## Notes

- **The detector's modules live in the plugin cache**, which a plugin update will wipe. There is
  now a `package.json` at `~/.claude/plugins/cache/impeccable/impeccable/4.1.1/` listing all five
  (four parsers plus puppeteer), so restoring them is one `npm install` — but if `detect.mjs`
  starts printing DEGRADED again, that is why.

- **Puppeteer uses the system Chrome** via `PUPPETEER_EXECUTABLE_PATH`, so nothing downloaded a
  browser. The width-pass and rasteriser scripts are in this session's scratchpad; say the word if
  you want them kept in the repo.

- **Three advisories are suppressed rather than fixed** in `deny.toml`, each with its reasoning and
  the condition that retires it: `rsa` (no upstream patch exists, and the sidechannel is on
  private-key operations this app never performs), `hickory-resolver` (pinned by jacquard to 0.24),
  and `atomic-polyfill` (unmaintained, not vulnerable). Worth re-checking on the next jacquard bump.
