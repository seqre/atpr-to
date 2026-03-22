# Pico CSS v2 — Forms

All form elements are full-width by default. Labels can wrap inputs (implicit association) or use `for`/`id` (explicit).

## Basic input with helper text

```html
<label for="email">
  Email address
  <input type="email" id="email" placeholder="you@example.com" aria-describedby="email-hint">
  <small id="email-hint">We'll never share your email with anyone.</small>
</label>
```

## Validation states

Use `aria-invalid` — not custom classes:

```html
<input type="text" aria-invalid="true">   <!-- error state -->
<input type="text" aria-invalid="false">  <!-- success state -->
```

Works the same on `<select>`, `<textarea>`, checkboxes, radios, and dropdowns.

## Input types

All styled consistently: `text`, `email`, `number`, `password`, `tel`, `url`, `date`, `datetime-local`, `month`, `time`, `search`, `color`, `file`

Common attributes: `disabled`, `readonly`, `required`, `placeholder`, `aria-label`, `aria-describedby`

## Textarea

Same patterns as `<input>`:

```html
<label>
  Message
  <textarea rows="4" placeholder="Your message…"></textarea>
</label>
```

## Select

```html
<label>
  Country
  <select>
    <option selected disabled value="">Choose…</option>
    <option>Canada</option>
    <option>United States</option>
  </select>
</label>

<!-- Multiple / size -->
<select multiple size="4">...</select>
```

## Checkboxes

```html
<fieldset>
  <legend>Preferences</legend>
  <label>
    <input type="checkbox" name="pref" checked>
    Option A
  </label>
  <label>
    <input type="checkbox" name="pref">
    Option B
  </label>
</fieldset>
```

Indeterminate state requires JavaScript: `checkbox.indeterminate = true`

## Radio buttons

```html
<fieldset>
  <legend>Plan</legend>
  <label>
    <input type="radio" name="plan" value="free" checked>
    Free
  </label>
  <label>
    <input type="radio" name="plan" value="pro">
    Pro
  </label>
</fieldset>
```

## Toggle switch

Transform a checkbox into a switch with `role="switch"` — pure CSS, no JS:

```html
<label>
  <input type="checkbox" role="switch" checked>
  Enable notifications
</label>
```

Supports `disabled` and `aria-invalid`.

## Range slider

```html
<label>
  Volume: <input type="range" min="0" max="100" value="50">
</label>
```

## Horizontal group (input + button inline)

Use `role="group"` on a `<fieldset>` to keep inputs and buttons on one line (stays horizontal on mobile, unlike `.grid`):

```html
<fieldset role="group">
  <input type="email" placeholder="Enter your email">
  <input type="submit" value="Subscribe">
</fieldset>
```

Search form variant:

```html
<form role="search">
  <input type="search" placeholder="Search…">
  <button type="submit">Go</button>
</form>
```

Button group with active state:

```html
<div role="group">
  <button>Day</button>
  <button aria-current="true">Week</button>
  <button>Month</button>
</div>
```

## Buttons

```html
<button>Primary</button>
<button class="secondary">Secondary</button>
<button class="contrast">Contrast</button>
<button class="outline">Outline primary</button>
<button class="outline secondary">Outline secondary</button>
<button class="outline contrast">Outline contrast</button>
<button disabled>Disabled</button>
```

`<input type="submit">` and `<input type="button">` are full-width by default. `<button>` is not.

`<input type="reset">` gets secondary styling automatically.

`<div role="button" tabindex="0">` also receives button styling.
