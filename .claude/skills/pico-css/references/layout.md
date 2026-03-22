# Pico CSS v2 — Layout

## Container

Centered, responsive fixed-width at breakpoints:

```html
<div class="container">...</div>
```

Full-width at all sizes:

```html
<div class="container-fluid">...</div>
```

| Breakpoint | Min viewport | Container width |
|---|---|---|
| xs | < 576px | 100% |
| sm | ≥ 576px | 510px |
| md | ≥ 768px | 700px |
| lg | ≥ 1024px | 950px |
| xl | ≥ 1280px | 1200px |
| xxl | ≥ 1536px | 1450px |

## Landmarks

`<header>`, `<main>`, `<footer>` as direct children of `<body>` receive automatic responsive vertical padding. `<section>` gets responsive `margin-bottom`. Combine with `.container` to control width:

```html
<body>
  <header class="container">...</header>
  <main class="container">...</main>
  <footer class="container">...</footer>
</body>
```

## Grid

Auto equal-column grid using CSS Grid. Columns collapse (stack vertically) on screens < 768px. No ordering, offsetting, or multi-breakpoint utilities — intentionally minimal.

```html
<div class="grid">
  <div>Column 1</div>
  <div>Column 2</div>
  <div>Column 3</div>
</div>
```

Number of columns is determined by the number of direct children. To create asymmetric layouts, nest grids or use CSS Grid directly.

## Overflow Auto

Wraps wide content (e.g., tables) to allow horizontal scrolling on narrow viewports:

```html
<div class="overflow-auto">
  <table>...</table>
</div>
```
