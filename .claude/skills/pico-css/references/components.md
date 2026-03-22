# Pico CSS v2 — Components

## Card

Built on `<article>` — no classes needed. `<header>` and `<footer>` get distinct background colors:

```html
<article>
  <header>Card header</header>
  <p>Card body content goes here.</p>
  <footer>Card footer</footer>
</article>
```

## Nav

Multiple `<ul>` elements inside `<nav>` distribute horizontally (flex). Inside `<aside>`, items stack vertically.

```html
<nav>
  <ul>
    <li><strong>Brand</strong></li>
  </ul>
  <ul>
    <li><a href="#">About</a></li>
    <li><a href="#" aria-current="page">Docs</a></li>
    <li>
      <!-- Dropdown in nav -->
      <details class="dropdown">
        <summary>More</summary>
        <ul>
          <li><a href="#">Settings</a></li>
          <li><a href="#">Logout</a></li>
        </ul>
      </details>
    </li>
  </ul>
</nav>
```

### Breadcrumbs

```html
<nav aria-label="breadcrumb">
  <ul>
    <li><a href="/">Home</a></li>
    <li><a href="/docs">Docs</a></li>
    <li>Current page</li>
  </ul>
</nav>
```

Customize the divider:

```css
nav[aria-label="breadcrumb"] {
  --pico-nav-breadcrumb-divider: "/";
}
```

## Accordion

Built on native `<details>`/`<summary>` — no JavaScript, no classes needed:

```html
<details open>
  <summary>Section 1</summary>
  <p>Content for section 1.</p>
</details>

<details>
  <summary>Section 2</summary>
  <p>Content for section 2.</p>
</details>
```

Exclusive group (only one open at a time) — use the `name` attribute:

```html
<details name="faq" open>
  <summary>Question 1</summary>
  <p>Answer 1</p>
</details>
<details name="faq">
  <summary>Question 2</summary>
  <p>Answer 2</p>
</details>
```

Button-style header:

```html
<details>
  <summary role="button" class="secondary">Open section</summary>
  <p>Content</p>
</details>
```

## Dropdown

Requires `.dropdown` class. Default is full-width; inside `<nav>` it sizes to content.

```html
<details class="dropdown">
  <summary>Choose an option</summary>
  <ul>
    <li><a href="#">Option A</a></li>
    <li><a href="#">Option B</a></li>
    <li><a href="#">Option C</a></li>
  </ul>
</details>
```

Button appearance:

```html
<details class="dropdown">
  <summary role="button" class="secondary">Actions</summary>
  <ul>
    <li><a href="#">Edit</a></li>
    <li><a href="#">Delete</a></li>
  </ul>
</details>
```

With checkboxes or radios inside:

```html
<details class="dropdown">
  <summary>Filter</summary>
  <ul>
    <li><label><input type="checkbox" checked> Active</label></li>
    <li><label><input type="checkbox"> Archived</label></li>
  </ul>
</details>
```

Right-to-left menu alignment: add `dir="rtl"` to the `<ul>`.

Validation state: `aria-invalid="true"` on `<details>`.

## Modal

Uses the native `<dialog>` element. Pico provides styling and CSS animations; open/close logic requires JavaScript.

```html
<!-- Trigger -->
<button id="open-modal">Open modal</button>

<!-- Modal -->
<dialog id="my-modal">
  <article>
    <header>
      <button aria-label="Close" rel="prev" id="close-modal"></button>
      <strong>Confirm action</strong>
    </header>
    <p>Are you sure you want to proceed?</p>
    <footer>
      <button class="secondary" id="cancel-modal">Cancel</button>
      <button id="confirm-modal">Confirm</button>
    </footer>
  </article>
</dialog>
```

JavaScript (add/remove classes on `<html>` for animations):

```js
const modal = document.getElementById("my-modal");

function openModal() {
  document.documentElement.classList.add("modal-is-opening", "modal-is-open");
  modal.showModal();
  setTimeout(() => document.documentElement.classList.remove("modal-is-opening"), 400);
}

function closeModal() {
  document.documentElement.classList.add("modal-is-closing");
  setTimeout(() => {
    document.documentElement.classList.remove("modal-is-open", "modal-is-closing");
    modal.close();
  }, 400);
}

document.getElementById("open-modal").addEventListener("click", openModal);
document.getElementById("close-modal").addEventListener("click", closeModal);
document.getElementById("cancel-modal").addEventListener("click", closeModal);
document.getElementById("confirm-modal").addEventListener("click", closeModal);
```

CSS classes on `<html>`:
- `.modal-is-open` — prevents background scroll
- `.modal-is-opening` — entry animation (400ms)
- `.modal-is-closing` — exit animation (400ms)

## Progress

```html
<!-- Determinate -->
<progress value="65" max="100"></progress>

<!-- Indeterminate (animated) -->
<progress></progress>
```

## Tooltip

Pure CSS, works on any element:

```html
<button data-tooltip="Save your changes">Save</button>
<a href="#" data-tooltip="More information" data-placement="right">Info</a>
```

`data-placement` values: `top` (default), `right`, `bottom`, `left`

## Loading spinner

`aria-busy="true"` on any element shows a spinner:

```html
<!-- Block element -->
<article aria-busy="true"></article>

<!-- Inline -->
<span aria-busy="true">Generating report…</span>

<!-- Button -->
<button aria-busy="true" aria-label="Please wait…">Submit</button>
```
