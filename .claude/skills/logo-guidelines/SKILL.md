---
name: logo-guidelines
description: Apply atpr.to logo and wordmark guidelines whenever producing material that carries the atpr.to mark — logos in slides, social posts, landing pages, README headers, mockups, SVG/PPTX deliverables. Trigger any time the user mentions "atpr.to" or "atpr" and asks for the logo, wordmark, brand lockup, or anything where the mark appears at brand scale. Does NOT govern site palette, body type, or component design — those live in DESIGN.md.
---

# atpr.to Logo & Wordmark

This skill governs the **logo, wordmark, and brand lockup**. It does not govern site-level color, type, or components — those are owned by `DESIGN.md` at the project root, which follows impeccable's design laws.

The mark is monochrome by design. That keeps it legible on any surface, prevents the logo from being conscripted as a decorative element, and lets the surrounding design system (which uses tinted neutrals and a restrained accent) breathe around it.

## Logo

Two variants, both monochrome:

- **Ink-on-paper**: dark mark on a light surface. Default.
- **Paper-on-ink**: light mark on a dark surface. Use only when the surface is genuinely dark.

The mark itself is rendered in pure black or pure white inside its bounding box — even when the surrounding site uses tinted neutrals. This is the one place pure `#000` / `#FFF` is canonical, because the mark needs to be a reproducible asset (SVG, favicon, social meta image) independent of the site's runtime palette.

**Always:**
- Use the official SVG file (`static/logo.svg` or the dark-mode counterpart), unmodified, at locked aspect ratio
- Match the variant to the background (ink on light, paper on dark)
- Keep clear space around the lockup of **at least the width of the "a" counter** on every side

**Never:**
- Stretch, squash, rotate, skew, or otherwise distort the mark
- Recolor the mark to the site's accent (or any other hue) — it stays pure ink or pure paper
- Add drop-shadows, glows, outlines, embosses, or any decorative effect
- Place the mark on a photographic or busy background — give it a clean field
- Modify or substitute any element of the mark

## Wordmark typography

When the word **atpr.to** appears as a wordmark (logo lockup, header bar, social post title, slide title), it is set in:

1. **Now Bold** (preferred)
2. **Inter Bold** (fallback)
3. **Helvetica Bold** (fallback)
4. System `sans-serif` bold (last resort — flag the substitution in handoff notes)

This applies to the wordmark lockup only. Regular body copy, headlines elsewhere in the site, and form labels follow `DESIGN.md`'s typography scale, which uses Now Bold for headlines but does not restrict body text.

## Site palette: not this skill's job

The atpr.to website uses a tinted-neutral palette with one restrained accent, defined in `DESIGN.md`. That palette deliberately avoids pure `#000` / `#FFF` for surfaces (warm off-white paper, warm near-black ink) and includes a single accent color used sparingly. The **logo** stays pure monochrome regardless — it sits *on* that palette but does not adopt it.

If a deliverable needs site-level color, type, or component decisions, consult `DESIGN.md`. If a deliverable is about the logo or wordmark lockup specifically, this skill applies.

## Before shipping logo deliverables

- Logo is the official SVG, unmodified, at correct aspect ratio?
- Variant matches the background (ink on light, paper on dark)?
- Clear space ≥ width of the "a" counter on all sides?
- Wordmark typography is Now Bold (or flagged fallback)?
- No effects, no recoloring, no rotation?
- Background is clean (not photographic, not busy)?

## Overrides

If the user explicitly asks for a logo treatment that departs from these rules (a holiday variant, a stamped/textured version for a specific campaign, etc.), flag it briefly ("heads up, this departs from the canonical lockup") then do it. This skill describes the canonical brand mark; the user can override per-deliverable.
