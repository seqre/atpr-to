/* Handle autocomplete on the sign-in field.
 *
 * The hardest moment on this page is typing `alice.bsky.social` exactly right,
 * from memory, on a phone. This turns recall into recognition.
 *
 * It only ever decorates the form. The form is a real POST to /api/login with
 * one field named `handle`; with this script blocked, failed or still loading,
 * typing a handle and pressing the button works exactly as before.
 */
(function () {
  "use strict";

  var ENDPOINT = "https://public.api.bsky.app/xrpc/app.bsky.actor.searchActors";
  var DEBOUNCE_MS = 150;
  var LIMIT = 5;

  var input = document.getElementById("handle");
  var list = document.getElementById("handle-results");
  if (!input || !list) return;

  var timer = null;
  var inflight = null;
  var items = [];
  var active = -1;

  function close() {
    list.hidden = true;
    list.innerHTML = "";
    items = [];
    active = -1;
    input.setAttribute("aria-expanded", "false");
    input.removeAttribute("aria-activedescendant");
  }

  function choose(handle) {
    input.value = handle;
    close();
    input.focus();
  }

  function highlight(next) {
    if (!items.length) return;
    if (active >= 0) items[active].el.classList.remove("is-active");
    active = (next + items.length) % items.length;
    var el = items[active].el;
    el.classList.add("is-active");
    el.scrollIntoView({ block: "nearest" });
    input.setAttribute("aria-activedescendant", el.id);
  }

  function render(actors) {
    list.innerHTML = "";
    items = [];
    if (!actors.length) {
      close();
      return;
    }

    actors.forEach(function (actor, i) {
      var li = document.createElement("li");
      li.className = "suggest-item";
      li.id = "handle-result-" + i;
      li.setAttribute("role", "option");
      li.setAttribute("aria-selected", "false");

      if (actor.avatar) {
        var img = document.createElement("img");
        img.className = "suggest-avatar";
        img.src = actor.avatar;
        img.alt = "";
        img.loading = "lazy";
        li.appendChild(img);
      } else {
        // Keeps the row rhythm identical whether or not there is a picture.
        var blank = document.createElement("span");
        blank.className = "suggest-avatar suggest-avatar--blank";
        li.appendChild(blank);
      }

      var text = document.createElement("span");
      text.className = "suggest-text";

      var handle = document.createElement("span");
      handle.className = "suggest-handle";
      handle.textContent = actor.handle;
      text.appendChild(handle);

      if (actor.displayName) {
        var name = document.createElement("span");
        name.className = "suggest-name";
        name.textContent = actor.displayName;
        text.appendChild(name);
      }

      li.appendChild(text);
      // `mousedown` rather than `click`: blur would close the list first.
      li.addEventListener("mousedown", function (e) {
        e.preventDefault();
        choose(actor.handle);
      });

      list.appendChild(li);
      items.push({ el: li, handle: actor.handle });
    });

    list.hidden = false;
    input.setAttribute("aria-expanded", "true");
    active = -1;
  }

  function search(q) {
    if (inflight) inflight.abort();
    inflight = new AbortController();

    fetch(ENDPOINT + "?q=" + encodeURIComponent(q) + "&limit=" + LIMIT, {
      signal: inflight.signal,
      headers: { accept: "application/json" },
    })
      .then(function (r) {
        return r.ok ? r.json() : null;
      })
      .then(function (body) {
        if (!body || !Array.isArray(body.actors)) return;
        render(body.actors);
      })
      .catch(function () {
        // Suggestions are a convenience. A failed lookup leaves the field
        // exactly as usable as it was, so it is not worth saying anything.
        close();
      });
  }

  input.addEventListener("input", function () {
    var q = input.value.trim();
    clearTimeout(timer);
    if (q.length < 2) {
      close();
      return;
    }
    timer = setTimeout(function () {
      search(q);
    }, DEBOUNCE_MS);
  });

  input.addEventListener("keydown", function (e) {
    if (list.hidden) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      highlight(active + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      highlight(active - 1);
    } else if (e.key === "Enter" && active >= 0) {
      // Only swallow the submit when a suggestion is actually selected.
      e.preventDefault();
      choose(items[active].handle);
    } else if (e.key === "Escape") {
      close();
    }
  });

  input.addEventListener("blur", function () {
    // After the mousedown handler has had its chance.
    setTimeout(close, 120);
  });

  /* ── the placeholder, cycling ─────────────────────────────────────────── */

  /* This page's whole claim is that *any* Atmosphere account works, not just a
     Bluesky one — and it made that claim only in the small print under the
     field, where a single `alice.bsky.social` placeholder was quietly saying
     the opposite. Rotating it through real servers puts the claim in the one
     place everybody looks.
   *
   * One name across four hosts, so the thing that changes is the only thing
   * that matters here: where the account lives. `alice` is the same
   * placeholder person throughout, which keeps the eye on the domain. */
  var EXAMPLES = [
    "alice.bsky.social",
    "alice.eurosky.social",
    "alice.blacksky.com",
    "alice.blacksky.community",
  ];
  var PERIOD_MS = 2600;

  var rotation = null;

  function stopRotating() {
    clearInterval(rotation);
    rotation = null;
  }

  function startRotating() {
    // Anyone who has touched the field is composing something; a placeholder
    // moving underneath the cursor is noise at exactly the wrong moment.
    if (input.value) return;

    var i = 0;
    rotation = setInterval(function () {
      i = (i + 1) % EXAMPLES.length;
      input.placeholder = EXAMPLES[i];
    }, PERIOD_MS);
  }

  // A placeholder that changes on its own is motion, and motion is a
  // preference. With it reduced, one example is picked and stays.
  if (!window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    startRotating();
    // Permanent: the field has been engaged with, and the teaching is done.
    input.addEventListener("focus", stopRotating, { once: true });
    input.addEventListener("input", stopRotating, { once: true });
  }
})();
