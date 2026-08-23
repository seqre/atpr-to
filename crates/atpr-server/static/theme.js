/* Rendition: galerie (light) and vitrine (dark).
 *
 * Loaded as a blocking classic script in <head>, which is deliberate. The old
 * shell did this inline; the policy is `script-src 'self'` with no
 * 'unsafe-inline', and CSP hashes were rejected because there is no build step
 * to regenerate one when a character changes. A blocking same-origin file is
 * one extra request and cannot silently break.
 *
 * Blocking matters: the attribute has to land before first paint, or the page
 * flashes the wrong rendition. Do not add defer or async.
 *
 * Only an explicit choice is stored. With nothing in localStorage the
 * attribute is left off entirely, so the CSS falls through to
 * prefers-color-scheme and the OS decides -- which is what someone who has
 * never touched the control almost always wants.
 */
(function () {
  "use strict";

  var KEY = "atpr-theme";
  var root = document.documentElement;

  function stored() {
    try {
      var v = localStorage.getItem(KEY);
      return v === "light" || v === "dark" ? v : null;
    } catch (e) {
      // Safari in private mode throws rather than returning null.
      return null;
    }
  }

  function apply(theme) {
    if (theme) {
      root.setAttribute("data-theme", theme);
    } else {
      root.removeAttribute("data-theme");
    }
  }

  // Before paint.
  apply(stored());

  function current() {
    return (
      stored() ||
      (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
    );
  }

  function toggle() {
    var next = current() === "dark" ? "light" : "dark";
    try {
      localStorage.setItem(KEY, next);
    } catch (e) {
      // Not persisting is survivable; not switching is not.
    }
    apply(next);
    label();
  }

  function label() {
    var btn = document.getElementById("rendition");
    if (!btn) return;
    var dark = current() === "dark";
    // The control says what it will do, not what is currently true.
    btn.setAttribute("aria-label", dark ? "Switch to the light rendition" : "Switch to the dark rendition");
    btn.setAttribute("aria-pressed", String(dark));
  }

  document.addEventListener("DOMContentLoaded", function () {
    var btn = document.getElementById("rendition");
    if (btn) btn.addEventListener("click", toggle);
    label();
  });

  // Follow the OS while the visitor has expressed no preference of their own.
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", function () {
      if (!stored()) label();
    });
})();
