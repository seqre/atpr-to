---
name: pico-css
description: Guide for building UIs with Pico CSS v2 (standard version). Covers correct semantic HTML patterns, layout, typography, forms, components, and customization. Use this skill whenever the user wants to build or style a page, component, or form with Pico CSS.
---

# Pico CSS v2 Guide

Pico CSS is a **semantic-first CSS framework**. Its philosophy:

- Style native HTML elements directly — the less you reach for classes, the better
- No JavaScript included — all components are pure CSS over native HTML semantics
- Responsive and dark/light mode out of the box

---

## Layout

See `references/layout.md` for full details. Quick reference:

```html
<!-- Fixed-width centered container -->
<div class="container">...</div>

<!-- Full-width container -->
<div class="container-fluid">...</div>

<!-- Auto equal-column grid (collapses < 768px) -->
<div class="grid">
  <div>Col 1</div>
  <div>Col 2</div>
  <div>Col 3</div>
</div>

<!-- Horizontal scroll wrapper for wide tables -->
<div class="overflow-auto"><table>...</table></div>
```

Breakpoints: xs < 576px, sm ≥ 576px, md ≥ 768px, lg ≥ 1024px, xl ≥ 1280px, xxl ≥ 1536px.

---

## Content (Typography)

See `references/content.md` for full details. All elements are responsive — no classes needed.

Key patterns:
- `<hgroup>` — collapses margins between headings; last child gets muted style
- `<blockquote>` with `<footer><cite>` — styled citation block
- Inline elements styled automatically: `<kbd>`, `<mark>`, `<del>`, `<ins>`, `<abbr>`, `<small>`, `<strong>`, `<em>`, `<sub>`, `<sup>`

---

## Forms

See `references/forms.md` for full details. All inputs are full-width by default.

Key patterns:
- Wrap `<input>` in `<label>` for implicit association — no `for`/`id` needed
- Use `aria-invalid="true"` for error state, `aria-invalid="false"` for valid
- Use `<small>` inside `<label>` for helper text (pair with `aria-describedby`)
- Use `<fieldset role="group">` or `<form role="search">` for horizontal input+button combos
- `<input type="checkbox" role="switch">` — toggle switch, no JS needed

---

## Components

See `references/components.md` for full details. Quick reference:

| Component | Element(s) | Class needed? |
|---|---|---|
| Card | `<article>` | No |
| Nav | `<nav><ul>...</ul></nav>` | No |
| Accordion | `<details>/<summary>` | No |
| Dropdown | `<details>/<summary>` | `.dropdown` |
| Modal | `<dialog>` | No (needs JS for open/close) |
| Progress | `<progress>` | No |
| Tooltip | any element | `data-tooltip=""` attribute |
| Loading spinner | any element | `aria-busy="true"` attribute |

---

## Customization

See `references/customization.md` for full details. Key CSS variables (all `--pico-` prefixed):

```css
:root {
  --pico-font-family: "Inter", sans-serif;
  --pico-font-size: 100%;
  --pico-border-radius: 0.5rem;
  --pico-spacing: 1rem;
  --pico-primary: #6750a4;
}
```

20 built-in color themes (Azure default), 380-color palette with utility classes like `pico-background-pink-500`.

---

## Common mistakes to avoid

- **Don't add classes where native HTML suffices.** `<article>` is a card. `<progress>` is a progress bar. `<details>/<summary>` is an accordion.
- **Buttons are only full-width when they're `<input type="submit/button">`** — not `<button>` elements.
- **No built-in JS** — modals need `dialog.showModal()`; Pico only styles them.
- **`aria-invalid`** drives validation styling, not custom classes.
- **`role="switch"`** on a checkbox makes a toggle; `role="group"` on a `<fieldset>` makes inputs inline.
