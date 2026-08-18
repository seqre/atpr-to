# Vendored faces

Both are self-hosted rather than loaded from a CDN, because the Content Security
Policy is `default-src 'self'` and there is no bundler to fetch them at build
time. Drop a file in here, rebuild, and `rust-embed` bakes it into the binary;
`mime_guess` already maps `.woff2` to `font/woff2`, so no Rust change is needed.

| File | Face | Axis | Role | Licence |
|---|---|---|---|---|
| `cinzel-latin.woff2` | [Cinzel](https://github.com/NDISCOVER/Cinzel) | weight 400–700 | Display, labels, short codes — letterspaced caps | OFL-1.1 |
| `jost-latin.woff2` | [Jost](https://github.com/indestructible-type/Jost) | weight 300–700 | Body, UI, tabular data | OFL-1.1 |
| `now-bold-latin.woff2` | Now Bold | 700 | The wordmark, and nothing else | supplied by the project owner |

Both are the **latin subsets** as served by Google Fonts, which is why they are
~26 KB each rather than several hundred. A handle or destination containing
characters outside that range falls back to the next face in the stack; that is
the trade for the size, and it is worth revisiting if it shows up in practice.

Licence texts are `OFL-Cinzel.txt` and `OFL-Jost.txt`, unmodified. Note
`cargo deny` never sees these files — it reads the Cargo dependency graph only —
so their licences are recorded here rather than checked anywhere.

Now Bold is the face `.claude/skills/logo-guidelines` names as the wordmark's
own type, and it is now here, so the substitution flag that used to sit in this
file is gone. It is loaded by exactly one rule — `.lockup-word` — and is not a
third UI face: Cinzel remains the display face for everything else, and is the
fallback behind Now in that one stack.

Unlike the other two it is not an open licence. It was supplied by the project
owner rather than fetched from a public source, and no licence text ships beside
it; self-hosting a commercial face in a public deployment is a webfont-licence
question for whoever runs the deployment. `cargo deny` reads the Cargo graph
only and has never seen any of these files.
