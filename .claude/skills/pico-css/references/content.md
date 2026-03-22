# Pico CSS v2 — Content & Typography

All typographic elements are fully responsive with a fluid scale. No classes needed.

## Font scale (xs → xxl breakpoints)

| Element | Min size | Max size |
|---|---|---|
| Body / H6 | 16px | 21px |
| H5 | 18px | 23.6px |
| H4 | 20px | 26.25px |
| H3 | 24px | 31.5px |
| H2 | 28px | 36.75px |
| H1 | 32px | 42px |
| `<small>` | 14px | 18.375px |

## Headings

Standard `<h1>`–`<h6>`. Use `<hgroup>` to group a heading with a subheading — collapses margins and mutes the last child:

```html
<hgroup>
  <h1>Page Title</h1>
  <p>Subtitle or description here</p>
</hgroup>
```

## Links

```html
<a href="#">Primary</a>
<a href="#" class="secondary">Secondary</a>
<a href="#" class="contrast">Contrast</a>
<a href="#" aria-current="page">Active page</a>
```

`aria-current="page"` marks the active link for both styling and assistive technology. Underlines appear on hover only by default.

## Blockquote

```html
<blockquote>
  "The best way to predict the future is to invent it."
  <footer>
    <cite>Alan Kay</cite>
  </footer>
</blockquote>
```

## Inline elements (all styled automatically)

| Element | Purpose |
|---|---|
| `<abbr title="...">` | Abbreviation with dotted underline |
| `<strong>`, `<b>` | Bold |
| `<em>`, `<i>` | Italic |
| `<del>` | Strikethrough (deleted text) |
| `<ins>` | Underline (inserted text) |
| `<s>` | Strikethrough (no longer accurate) |
| `<kbd>` | Keyboard input styling |
| `<mark>` | Highlighted text |
| `<small>` | Fine print / helper text |
| `<sub>`, `<sup>` | Subscript / superscript |
| `<u>` | Underline (annotation) |
| `<cite>` | Citation / title |

## Horizontal rule

```html
<hr>
```

Styled as a subtle separator — no classes needed.

## Tables

```html
<table>
  <thead>
    <tr><th>Name</th><th>Value</th></tr>
  </thead>
  <tbody>
    <tr><td>Alpha</td><td>1</td></tr>
    <tr><td>Beta</td><td>2</td></tr>
  </tbody>
  <tfoot>
    <tr><td>Total</td><td>3</td></tr>
  </tfoot>
</table>
```

Add `.striped` to `<tbody>` for alternating row colors:

```html
<tbody class="striped">...</tbody>
```

Wrap in `<div class="overflow-auto">` for horizontal scroll on narrow screens.
