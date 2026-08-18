---
name: atpr.to
description: A pierced lattice of squares, ruled in lacquer black on gallery white — ownership shown as a material fact.
colors:
  ground: "#f0eee8"
  plate: "#f7f6f1"
  ink: "#0d0d0c"
  ink-soft: "#4a4843"
  ink-faint: "#625f58"
  hair: "#0d0d0c26"
  silver-mid: "#bab7b0"
  silver-lo: "#87847d"
  signal: "#9b2c1e"
  signal-ground: "#9b2c1e14"
typography:
  wordmark:
    fontFamily: "Now, Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "21px"
    fontWeight: 700
    lineHeight: 1.55
    letterSpacing: "0.03em"
  display:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "clamp(28px, 5.2vw, 56px)"
    fontWeight: 600
    lineHeight: 1.08
    letterSpacing: "0.16em"
  headline:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "clamp(17px, 2.4vw, 21px)"
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: "0.2em"
  title:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "14px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "0.14em"
  body:
    fontFamily: "Jost, system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "normal"
  lede:
    fontFamily: "Jost, system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "clamp(16px, 1.6vw, 18px)"
    fontWeight: 300
    lineHeight: 1.55
    letterSpacing: "normal"
  note:
    fontFamily: "Jost, system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "normal"
  label:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "11px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "0.24em"
  control:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "13px"
    fontWeight: 700
    lineHeight: 1.35
    letterSpacing: "0.16em"
  meta:
    fontFamily: "Jost, system-ui, -apple-system, 'Segoe UI', sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.3
    letterSpacing: "normal"
  code:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "15px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "0.1em"
  address:
    fontFamily: "Cinzel, Georgia, 'Times New Roman', serif"
    fontSize: "clamp(23px, 3.6vw, 44px)"
    fontWeight: 600
    lineHeight: 1.05
    letterSpacing: "0.02em"
  data:
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.85
    letterSpacing: "normal"
rounded:
  none: "0"
spacing:
  sq: "5px"
  rule: "1.5px"
  control: "50px"
  s1: "12px"
  s2: "24px"
  s3: "36px"
  s4: "48px"
  s6: "72px"
  s9: "108px"
components:
  button:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "11px 24px"
    height: "40px"
  button-hover:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ground}"
  button-fill:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "11px 24px"
    height: "40px"
  button-fill-hover:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.signal}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "11px 24px"
    height: "40px"
  button-danger-hover:
    backgroundColor: "{colors.signal}"
    textColor: "{colors.ground}"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.none}"
    padding: "6px"
    size: "15px"
  field:
    backgroundColor: "{colors.plate}"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "11px 12px"
  tab:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.ground}"
    typography: "{typography.label}"
    rounded: "{rounded.none}"
    padding: "5px 12px 4px"
  state:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    typography: "{typography.body}"
    rounded: "{rounded.none}"
    padding: "10px 24px"
  state-error:
    backgroundColor: "{colors.signal-ground}"
    textColor: "{colors.signal}"
    rounded: "{rounded.none}"
    padding: "10px 24px"
  cell:
    backgroundColor: "transparent"
    rounded: "{rounded.none}"
    size: "14px"
  cell-on:
    backgroundColor: "{colors.ink}"
    rounded: "{rounded.none}"
    size: "14px"
  qr-plate:
    backgroundColor: "#ffffff"
    textColor: "#000000"
    rounded: "{rounded.none}"
    padding: "24px"
    width: "240px"
---

# Design System: atpr.to

## Overview

**Creative North Star: "The Gridsmith"**

A Wiener Werkstätte pierced lattice, built as a real object rather than a page decoration. One module — a single cell — generates every measurement in the system; squares either fill solid or stay pierced open, rules are drawn in lacquer black at a hairline weight, and nothing is round, soft, or floated. The world's argument is structural: a short link here is a record its author owns, so the interface is made of parts that visibly count, join, and lock rather than parts that glow.

The register is bolder than a utility shortener usually risks. Cinzel capitals run letterspaced at poster scale on the landing address and the preview link — size is billing, and the link is the subject of its own page — while Jost carries every sentence, field, and dense row beneath it. Density is deliberately high in the working surfaces (the dashboard rack, the live wall) and deliberately open in the reading surfaces (the landing, the preview, the error page), and both are the same grid at different counts. Two renditions ship: **Galerie**, gallery white ruled in black, and **Vitrine**, the same objects in a case at night. Vitrine is not an inversion and not a mirror — ink and ground swap, but the grey ramp between them is measured separately for each, because a dark ground swallows a dim grey that a light ground carries.

Confirmed rejections, from the direction contract the build carries in its own markup: no centred paste-box on white, and no terminal green. There is no decorative accent anywhere; the one chromatic value in the palette is reserved for destruction and error, and spending it on anything else empties it.

**Verified, and the record says how.** Every surface has been seen running in Chrome against a real PDS — landing, dashboard with real links in the rack, QR dialog, link preview, error pages — and the widths have now been *measured* rather than asserted. A headless Chrome loaded all four surfaces at 320 / 375 / 390 / 768 / 1024 / 1440 in both renditions: no page scrolls sideways, nothing escapes its container, no letterspaced capital falls below the 11px floor, and the smallest visible control is 29px square. That pass earned its keep by finding two defects — the dashboard masthead laid out 344px wide inside a 320px viewport, taking the rendition toggle off-screen, and the sign-out button was being squeezed to 18px through the form wrapping it.

The greys are measured, not judged: the faint step had been failing AA in both renditions at 4.03:1 and 4.25:1, and the whole ramp was rebuilt around a 5:1 floor (see Colors). Interaction was exercised live: sign-in through a real authorization server, create, copy, repoint, QR, filter, sort, the overflow menu, and the rendition toggle restoring before first paint. Zero CSP violations, and all four cross-origin endpoints in `connect-src` confirmed working under the live policy.

**What is still not verified:** no real device, no touch input, and no browser other than Chrome. The `(pointer: coarse)` rules that raise controls to 44px are therefore unexercised — the measurements above are all from a fine pointer. The design detector now runs with its parsers present rather than degraded, and reports nothing across the stylesheet, the templates, the scripts, and a 390px URL scan.

**Key Characteristics:**
- One module (`--cell`) generates the entire spacing scale; there is no second scale
- Zero corner radius everywhere, including focus rings and dialogs
- Squares that fill or stay open are the system's only ornament
- Two renditions, neither an inversion of the other
- One chromatic value, semantic only
- One authored motion, used at every scale
- Hand-written CSS in a single file; no framework, no bundler, no build step

## Colors

A warm achromatic world — bone-white ground, lacquer-black ink, a metal ramp between them — interrupted by exactly one oxide red that only ever means damage.

### Primary
- **Lacquer Black** (`{colors.ink}`): The system's structural material. Every rule, border, focus ring, filled cell, section tab, and filled button ground. It is not "text colour that happens to be dark"; it is the line the whole lattice is drawn in.
- **Galerie Bone** (`{colors.ground}`): The page ground, and the reverse-out colour on every inked surface. Warm rather than neutral white, so the black reads as lacquer on paper instead of as a screen.

### Secondary
- **Oxide Red** (`{colors.signal}`): Destructive actions and error states only — the danger button, its icon-button sibling, the error state line, and the wash behind it (`{colors.signal-ground}`). It appears on no other surface.

### Tertiary
- **Metal Grey** (`{colors.silver-mid}`, with `{colors.silver-lo}` as its shadow): What remains of the silver. It dresses furniture and only furniture — the scrollbar thumb and the fill behind an avatar that did not load. In Vitrine it brightens (`#c2beb4 / #98948b`) rather than darkening with the ground. It is not a surface for content and never a control: the hammered material it came from was retired when the one button wearing it gave it up.

### Neutral
- **Plate** (`{colors.plate}`): The inset surface — input grounds, the suggestion list, the record sheet on the landing page. One step off the ground, never a card with a shadow.
- **Soft Ink** (`{colors.ink-soft}`): Secondary prose — ledes, destinations in a row, timestamps at reading size, the record body. `#4a4843` in Galerie, `#c4c0b6` in Vitrine.
- **Faint Ink** (`{colors.ink-faint}`): Tertiary metadata — placeholders, counts, address separators, "when" columns. The quietest legible register, and *legible* is the operative word: it carries 11–13px text, so it is held to normal-text AA rather than the large-text exemption. `#625f58` in Galerie (5.89:1 on plate), `#9a968d` in Vitrine (6.01:1). Both replaced values that measured about 4:1 and failed.
- **Hair** (`{colors.hair}`): The internal divider, ink at ~15% alpha. Rows inside a ruled container are separated by hair; the container itself is separated by a full rule.

### Named Rules

**The One Signal Rule.** Oxide red means destruction or failure. It is never a brand accent, never a hover, never a highlight, never a link colour. A screen with nothing broken on it has no red on it.

**The Complete Palette Rule.** Every colour is declared on bare `:root` first and only *redefined* under a rendition selector. A visitor on "system" with no stored choice must land on a complete palette, never a partial one.

**The Unmirrored Grey Rule.** Vitrine is not an inversion, and its greys are not the Galerie greys flipped. A dark ground eats low-contrast type in a way a light ground does not, so both dim steps sit lighter than symmetry would put them: the Vitrine ramp measures 14.86 / 9.76 / 6.01 against its plate, roughly a factor of 1.5 at each step, and every value in it was measured rather than picked. The metal greys move the same way and for a different reason — in a night vitrine the metal is the light source rather than a surface taking light, so it brightens instead of following the ground down. Never generate the dark rendition by inverting the light one, and never mirror a grey ramp across the two.

**The Pinned Ink Rule.** The QR container pins pure black on pure white in *both* renditions. The QR fragment paints in `currentColor` on a transparent ground precisely so its container decides — a light-on-dark code fails on a meaningful share of scanners, and consistency with the surrounding page is not worth a link that will not scan.

## Typography

**Wordmark Font:** Now Bold — vendored latin subset, weight 700. It sets the wordmark and nothing else; exactly one rule names it, and Cinzel stands behind it in that one stack. Unlike the other two it is not an open licence (see `static/fonts/README.md`).
**Display Font:** Cinzel (with Georgia, Times New Roman, serif) — vendored latin subset, weight axis 400–700
**Body Font:** Jost (with system-ui, -apple-system, Segoe UI, sans-serif) — vendored latin subset, weight axis 300–700
**Data Font:** the platform monospace stack (ui-monospace, SFMono-Regular, Menlo, Consolas)

**Character:** Roman inscriptional capitals over a geometric grotesque — engraved authority for anything that names a thing, plain modern clarity for anything that explains it. The pairing is strictly divided by role, never by taste.

### Hierarchy
- **Display** (600, `clamp(28px, 5.2vw, 56px)`, 1.08, 0.16em, uppercase): Page headline. On the landing it is the *address itself* — `atpr.to / @handle / code` — set at poster scale with each part labelled where it sits.
- **Address / Preview Link** (600, `clamp(23px, 3.6vw, 44px)` and `clamp(24px, 4.4vw, 50px)`, 0.02em, mixed case): The two places where the display face drops its uppercasing and nearly all its tracking, because it is rendering a literal URL that must read as one.
- **Headline** (600, `clamp(17px, 2.4vw, 21px)`, 1.25, 0.2em, uppercase): Sub-headings within a surface.
- **Title** (600, 14px, 1.35, 0.14em, uppercase): Empty-state and dialog headings.
- **Body** (400, 16px, 1.55): All prose and all input text. Paragraphs cap at 68ch.
- **Lede** (300, `clamp(16px, 1.6vw, 18px)`): The one paragraph under a display headline. Light weight, soft ink.
- **Note** (400, 13px, faint ink, 62ch): Hints, disclaimers, empty-state prose.
- **Label** (600, 11px, 0.24em, uppercase, display face): Field labels, fact labels, counts. The floor of the display face.
- **Control** (700, 13px, 0.16em, uppercase, display face): Button labels. Two steps above the label floor and at the top of Cinzel's weight axis, because a control has to be read at a glance and pressed correctly the first time. It sat at 12px/600/0.2em and was hard to read: letterspacing that opens a label up also thins it out, so the size and weight have to pay for the tracking.
- **Meta** (12px, body face, unspaced, faint ink): Secondary metadata — a suggestion's display name, a row's timestamp. Nothing a visitor must read to understand the page sits here.
- **Short Code** (600, 15px, 0.1em–0.12em, uppercase, display face): A user's chosen code, wherever it appears — rack row, wall row, preview fact.
- **Data** (400, 13px, 1.85, monospace): The literal atproto record on the landing page, and inline `to.atpr.link` mentions. The one place a face outside the system is allowed, because it is showing machine text as machine text.
- **Wordmark** (700, 21px, 0.03em, Now Bold, lowercase): The lockup, and only the lockup. It takes far less tracking than the display face because it is a geometric sans and not inscriptional capitals — the air Cinzel needed at this size closes up here.

### Named Rules

**The Two Roles Rule.** Cinzel sets display, labels, and short codes — anything that *names*. Jost sets body, UI, and dense data — anything that *explains* or *lists*. A destination URL, a handle, and a timestamp are Jost; a short code is Cinzel. There is no third assignment.

**The 11px Floor Rule.** Letterspaced capitals never go below 11px, and tracking eases in as size falls (0.24em at 11px, 0.16em at display size). Below the floor the tracking closes and the caps become noise.

**The Tabular Rule.** Anything countable or comparable carries tabular numerals (`.num`): counts, statuses, timestamps.

## Layout

The whole spatial system is one module. `--cell` is 12px, and every space in the stylesheet is `--cell` × {1, 2, 3, 4, 6, 9} — 12/24/36/48/72/108px. There is no second spacing scale and no arbitrary value; the few literals that exist (5px square unit, 1.5px rule, 7px joint, 14px cell) are the lattice's own drawing units, not spacing.

**The module steps; it never scales.** At ≤1023px the cell becomes 11px, at ≤767px 10px (and the square unit drops 5px → 4px), at ≤479px 9px. Nothing is fluid — the lattice re-counts its squares as the viewport narrows, it never stretches one.

The shell is a single centred column at max 1180px with `36px 24px 72px` padding. Surfaces are two-column asymmetric grids that collapse to one:
- **Landing hero** — `1.35fr / 1fr` (argument + sign-in, live wall), 72px gutter, collapsing at 899px with sign-in first, because on a phone the visitor came to do something.
- **Record grid** — `1fr / 1.1fr`, 48px gutter, collapsing at 899px.
- **Preview** — `1fr / auto` (facts, QR), 72px gutter, collapsing at 899px.
- **Make form** — `1fr / 16ch / auto` (destination, code, submit), 12px gutter; the destination gets the room because that is what gets pasted. One column at 767px.
- **Rack row** — `auto / 1fr / auto / auto`, reflowing at 767px to a two-line named-area grid (`cell body` / `when acts`).

Section rhythm is asymmetric on purpose: 108px above a section, 24px below its heading row. Vertical stacks are 24px (`.stack`) or 12px (`.stack-s`).

### Named Rules

**The One Module Rule.** Every space is a multiple of the cell. If a value cannot be expressed as `var(--s1..s9)`, it is not a space — it is a drawing unit and must be justified as one.

**The Re-Count Rule.** Responsive change happens by counting differently, not by scaling. Step the module at a breakpoint; never make it fluid.

**The Contained Scroll Rule.** Wide content scrolls inside its own container (`.scroll-x`); the page body never scrolls sideways. Long destinations truncate with an ellipsis and never wrap the layout.

## Elevation & Depth

**There are no shadows in this system.** Not one `box-shadow` ships. Depth is entirely material and tonal: a full rule (1.5px lacquer black) separates an object from the page, a hair rule (ink at ~15%) separates rows inside an object, and the plate tone lifts an inset surface one step off the ground. A modal is a ruled rectangle on a near-opaque black backdrop (`#0d0d0cad`) — it reads as forward because everything behind it is extinguished, not because it floats.

There is now no exception. A hammered-silver material — directional ramp, specular bloom, planished dimpling, noise tooth — used to dress one button, the QR dialog's download action. It was retired: a dialog's secondary action is the quietest control on any surface, and the loudest material in the system was sitting on it. What survives is the metal grey on the scrollbar thumb and the blank-avatar fill, where it is furniture rather than a surface anyone reads.

### Named Rules

**The One-Instance Rule.** A material with a single instance is a claim, not a system: the next surface needing that role either honours it or the material goes. This system has now run that test once and lost — the hammered silver had exactly one button, the button did not want it, and the material was retired rather than kept alive as thirty lines of CSS nothing reaches. Do not reintroduce a material for one element.

**The No-Shadow Rule.** Objects are separated by rules and tone. Never add a drop shadow, a glow, a blur, or a lifted card. There is no light source in this system at all.

**The Full Rule / Hair Rule.** A container's own edge is a full rule in ink. Divisions *inside* a container are hair. Mixing them flattens the hierarchy the system uses instead of elevation.

## Shapes

Zero radius, everywhere, without exception — buttons, fields, dialogs, avatars, focus rings. The square is the system's only form, and it appears at four scales: the 5px unit that draws the pierced edge, the 7px joint in a divider row, the 14–15px cell in a row or an icon, and the 22–24px cell in a lattice.

**The pierced frame** is the signature device: a run of solid squares along all four edges of a container, drawn as *four repeating linear gradients* rather than a border-image — specifically so it re-counts its own squares as the container narrows, with no per-breakpoint work. It inherits its colour from `--f` and pads its contents by one cell.

**The four-square mark** (four 5×5 squares in a 15×15 box, painted in `currentColor`) is the product's own glyph. It marks *atpr.to* and nothing else: the cap inside a field, the affordance inside a filled button. It is deliberately not on any icon button, because a mark that also means "control" no longer means the product.

**Action glyphs** are drawn on the same 15×15 grid, from axis-aligned rectangles in `currentColor`, one per action: two overlapping plates (copy), a shaft with a three-step head (repoint), three finder squares and two loose cells (QR), five cells on the two diagonals (delete), and a pair of solid cells beside a pair of pierced ones (the rendition toggle). No icon set, no icon font — every glyph in this system is drawn from the lattice the system is made of.

**The divider** is never a bare line: a full rule with a solid square joint at its start, and an inverted black tab naming the section at its head.

### Named Rules

**The No-Radius Rule.** Nothing in this world is rounded, including focus outlines and images. If a corner needs softening, the composition is wrong.

**The Pierced-Gradient Rule.** The pierced frame is always four repeating gradients. Never re-implement it as a border-image, an SVG frame, or a fixed sprite — those stretch a square instead of dropping one.

## Components

### Buttons
- **Shape:** Perfectly square (0 radius), 1.5px ink rule, 40px minimum height, 11px × 24px padding, 12px letterspaced Cinzel caps (0.2em). Never underlined — three of these controls are anchors, and the link default has no business on a button.
- **Beside a field:** a button standing next to an input takes `{spacing.control}` (50px) instead of the 40px floor, so the pair reads as one control. That height is measured from the field — 16px type at 1.55 over 11px of padding inside a rule — and is the one value in the system that does not step with the module, because none of its three parts may shrink.
- **Default (ghost):** Transparent ground, ink rule, ink text. Hover inverts to a solid ink ground with reverse-out text (240ms on the system ease).
- **Filled:** Solid ink ground, reverse-out text. Hover inverts the *other* way, back to transparent — the two variants trade places rather than darkening.
- **Danger:** Oxide rule and text; hover fills oxide with reverse-out text. Used only for delete.
- **Disabled:** 34% opacity, `not-allowed` cursor, hover suppressed.
- **Coarse pointer:** minimum height rises to 44px.
- **Icon button:** a 15px action glyph — never the atpr.to mark — with a hair rule at rest, filling to solid ink on hover; padding rises 6px → 14px under a coarse pointer. Four of them sit in a dashboard row, so they must be four different glyphs: a tooltip is not a substitute for being able to tell two controls apart without stopping.

### Cards / Containers
- **Corner Style:** Square, always.
- **Background:** Ground by default; plate for inset sheets (`.plate`).
- **Border:** `.ruled` — 1.5px ink. `.pierced` for the ceremonial variant, reserved for moments that deserve a frame (the empty rack).
- **Shadow Strategy:** None. See Elevation & Depth.
- **Internal Padding:** One or two cells (12/24px); a modal takes three (36px).

### Inputs / Fields
- **Style:** A ruled box on plate, with the input itself borderless inside it and a squared cap segment (rule-separated) carrying the four-square mark. 16px text — below that iOS zooms the viewport on focus.
- **Focus:** The whole field takes a 1.5px ink outline at 2px offset; the inner input has no outline of its own, so the control focuses as one object.
- **Placeholder:** Faint ink.
- **Suggestions:** Anchored to the field, overlapping its rule by exactly the rule width so it reads as an extension of the control rather than an overlay. Rows separated by hair; the active row inverts to solid ink.

### Navigation
- **Masthead:** The lockup (34px mark + wordmark) at one end, identity and controls at the other, closed by a full ink rule beneath. No nav links; the surface set is small enough that the lockup is the only navigation.
- **Rendition toggle:** An icon button carrying the solid/pierced pair — the world's own duality standing for the two renditions — labelled with what it will *do*, not what is currently true.
- **Skip link:** Visually hidden until focused, then a solid ink block with reverse-out text. A skip link that never appears does not work.

### State Lines
States print themselves in the system's own voice: a ruled row with a Cinzel label and a plain-Jost sentence. Errors take the oxide rule, oxide text, and the oxide wash. There are no toasts, no alerts, and no foreign modal chrome for something that is only information.

### The Cell and the Strike (signature)
The struck cell is the system's central object. A cell (`.cellsq`) is a square with an ink rule and a transparent ground; a *struck* cell (`.on`) is filled solid. That is the whole vocabulary for "a record exists": a filled square in a rack row, a filled square in a live wall row, four squares on the error page saying a thing did not land, a ten-cell lattice in the empty state.

**The strike** is the one authored motion in the system: 620ms on an exponential ease-out (`cubic-bezier(0.16, 1, 0.3, 1)`), scaling 0.42 → 1.06 → 1 while the ground goes transparent → ink. It plays when a record is written — on the dashboard when you write your own, on the wall when the network writes one — at every scale, orchestrated rather than scattered across hovers. Under `prefers-reduced-motion` it reduces to a state change.

### QR Plate (signature)
A QR code is a pierced square lattice in black and white — the same object this design is built from — so it is the preview page's hero material at 240px, not a utility in a corner. The plate pins `#fff`/`#000` in both renditions and never drops below ~180px on a narrow viewport.

## Do's and Don'ts

### Do:
- **Do** derive every space from the module (`12/24/36/48/72/108px` at desktop), and step the module at a breakpoint rather than making it fluid.
- **Do** draw the pierced frame as four repeating gradients so it re-counts its squares.
- **Do** keep the two type roles absolute: Cinzel names things, Jost explains them.
- **Do** hold letterspaced capitals at 11px or larger, easing tracking in as size falls.
- **Do** separate containers with a full ink rule and their internal rows with hair.
- **Do** define every colour on bare `:root` first, then redefine only what changes per rendition.
- **Do** pin black-on-white inside the QR container in both renditions.
- **Do** use the struck cell — and only the struck cell — to say a record exists.
- **Do** restore the touch floor under a coarse pointer (44px controls, 14px icon-button padding).
- **Do** keep all CSS in `static/app.css` and all script same-origin: `default-src 'self'` with no `unsafe-inline` and no `unsafe-eval` is a hard constraint, and there is no bundler to generate a hash.

### Don't:
- **Don't** spend the oxide red on anything but destruction and error. It is not an accent.
- **Don't** add a shadow, glow, blur, or lifted card. Depth is rules and tone.
- **Don't** round a corner — not on a control, a dialog, an avatar, or a focus ring.
- **Don't** build the dark rendition by inverting the light one, and don't mirror a grey ramp across the two — Vitrine's dim steps sit lighter than symmetry would put them, and the values are measured.
- **Don't** put the metal grey on anything a person reads. It is furniture: a scrollbar thumb, a missing avatar.
- **Don't** introduce a second spacing scale, a second ease, or a second animation. One module, one curve (`cubic-bezier(0.16, 1, 0.3, 1)` at 240ms/620ms), one strike.
- **Don't** add an icon set or icon font. Glyphs are drawn here, from axis-aligned rectangles on a 15×15 grid in `currentColor`: two plates for copy, a stepped arrow for repoint, three finder squares for QR, five cells on the diagonals for delete, and a solid-versus-pierced pair for the rendition toggle. The logo mark is monochrome by rule — it never takes the signal colour and is never recoloured, distorted, or given effects.
- **Don't** reuse the four-square atpr.to mark as a button glyph. It means the product, and it stops meaning that the moment it also means "control" — it belongs on the field caps and nowhere a person clicks.
- **Don't** stack the section tab above a heading as a kicker or eyebrow. The tab *is* the section heading; it never introduces another one.
- **Don't** use a toast, an `alert()`, or borrowed modal chrome to report a state. States print as ruled lines in the system's own voice.
- **Don't** inline a style attribute or a script tag; the CSP forbids it and there is no build step to hash one.

<!-- Not canonized. Nothing outstanding here: the wordmark's flagged substitution was resolved when Now Bold was supplied, and it is now the only face `.lockup-word` names. -->
