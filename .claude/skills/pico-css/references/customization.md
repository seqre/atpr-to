# Pico CSS v2 — Customization

## CSS variables

All variables are prefixed with `--pico-`. Override in `:root` or any element scope.

### Global

```css
:root {
  --pico-font-family: system-ui, sans-serif;
  --pico-font-family-monospace: "Fira Code", monospace;
  --pico-font-size: 100%;           /* scales to 131.25% at 1536px+ */
  --pico-line-height: 1.5;
  --pico-font-weight: 400;
  --pico-border-radius: 0.25rem;
  --pico-border-width: 0.0625rem;
  --pico-outline-width: 0.125rem;
  --pico-transition: 0.2s ease-in-out;
  --pico-spacing: 1rem;             /* base unit for padding/margin */
}
```

### Colors

```css
:root {
  /* Theme colors */
  --pico-primary: #0172ad;
  --pico-secondary: #5d6b89;
  --pico-contrast: #181c25;
  --pico-muted-color: #646b79;

  /* Surfaces */
  --pico-background-color: #fff;
  --pico-color: #373c44;
  --pico-card-background-color: #fbfbfc;
  --pico-card-border-color: #e7eaf0;
}
```

Dark mode values set on `[data-theme="dark"]` override the same variables.

### Icons (SVG data URLs)

```css
:root {
  --pico-icon-checkbox: url("...");
  --pico-icon-chevron: url("...");
  --pico-icon-date: url("...");
  --pico-icon-time: url("...");
  --pico-icon-search: url("...");
  --pico-icon-close: url("...");
  --pico-icon-loading: url("...");
  --pico-icon-valid: url("...");
  --pico-icon-invalid: url("...");
}
```

## Color themes (Sass)

20 preset themes: Red, Pink, Fuchsia, Purple, Violet, Indigo, **Blue (default: Azure)**, Cyan, Jade, Green, Lime, Yellow, Amber, Pumpkin, Orange, Sand, Grey, Zinc, Slate.

```scss
@use "pico" with (
  $theme-color: "purple",
);
```

## Palette utilities

380 hand-crafted colors across 20 families × 19 shades (50–950).

CSS variable: `--pico-pink-500`

Utility classes:

```html
<p class="pico-color-pink-500">Colored text</p>
<div class="pico-background-indigo-100">Colored background</div>
```

Sass:

```scss
@use "@picocss/pico/scss/colors" as *;

.my-element {
  color: $pink-500;
  background: $indigo-100;
}
```

## Sass configuration

```scss
@use "pico" with (
  $theme-color: "azure",               // color theme
  $enable-semantic-container: false,   // auto-container on body children
  $enable-classes: true,               // variant classes (.secondary etc.)
  $enable-responsive-typography: true,
  $enable-viewport: true,
  $css-var-prefix: "pico",             // customize --pico- prefix
  $modules: (
    "components/accordion": true,
    "components/modal": true,
    "components/dropdown": true,
    "components/loading": true,
    "components/modal": false,         // set false to exclude and reduce bundle
  ),
);
```

Disabling unused modules can reduce file size by ~50%.

## Scoping to a container

Use the conditional bundle (`pico.conditional.min.css`) to scope all Pico styles under a `.pico` wrapper, leaving the rest of your page unstyled:

```html
<link rel="stylesheet" href=".../pico.conditional.min.css">
<div class="pico">
  <!-- Pico styles only apply inside here -->
</div>
```

Or configure via Sass:

```scss
@use "pico" with (
  $parent-selector: ".pico",
);
```

## RTL support

Set `dir="rtl"` on `<html>` or any element — layout, alignment, and directional properties adjust automatically:

```html
<html dir="rtl" lang="ar">
```

Or per-element:

```html
<blockquote dir="rtl" lang="ar">نص عربي</blockquote>
```
