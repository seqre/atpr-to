# Vendored faces

Both are self-hosted rather than loaded from a CDN, because the Content Security
Policy is `default-src 'self'` and there is no bundler to fetch them at build
time. Drop a file in here, rebuild, and `rust-embed` bakes it into the binary;
`mime_guess` already maps `.woff2` to `font/woff2`, so no Rust change is needed.

| File | Face | Axis | Role | Licence |
|---|---|---|---|---|
| `cinzel-latin.woff2` | [Cinzel](https://github.com/NDISCOVER/Cinzel) | weight 400–700 | Display, labels, short codes — letterspaced caps | OFL-1.1 |
| `jost-latin.woff2` | [Jost](https://github.com/indestructible-type/Jost) | weight 300–700 | Body, UI, tabular data | OFL-1.1 |

Both are the **latin subsets** as served by Google Fonts, which is why they are
~26 KB each rather than several hundred. A handle or destination containing
characters outside that range falls back to the next face in the stack; that is
the trade for the size, and it is worth revisiting if it shows up in practice.

Licence texts are `OFL-Cinzel.txt` and `OFL-Jost.txt`, unmodified. Note
`cargo deny` never sees these files — it reads the Cargo dependency graph only —
so their licences are recorded here rather than checked anywhere.

Neither face is Now Bold, which `.claude/skills/logo-guidelines` names as the
wordmark's own type. No Now file exists in this repository, so the wordmark
ships on a fallback; the skill requires that substitution be flagged rather than
made quietly, and this is the flag.
