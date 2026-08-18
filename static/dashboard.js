/* The dashboard.
 *
 * Ranked by how often it happens, not by how interesting it is: write a link
 * and get it onto the clipboard; find one and check where it points; repair
 * one. The first is the reason anybody is here, so creating a link puts it on
 * the clipboard without being asked and says so.
 *
 * Every error body on every route is {"error": "..."} with a JSON content
 * type, which is why there is one `explain` here and not a special case per
 * call site.
 */
(function () {
  "use strict";

  var PAGE = 50;

  var rack = document.getElementById("rack");
  if (!rack) return;

  var makeForm = document.getElementById("make");
  var urlInput = document.getElementById("url");
  var codeInput = document.getElementById("code");
  var makeError = document.getElementById("make-error");
  var filter = document.getElementById("filter");
  var loading = document.getElementById("rack-loading");
  var rackError = document.getElementById("rack-error");
  var empty = document.getElementById("rack-empty");
  var nomatch = document.getElementById("rack-nomatch");
  var moreWrap = document.getElementById("rack-more");
  var moreBtn = document.getElementById("more");
  var count = document.getElementById("rack-count");

  var confirmDlg = document.getElementById("confirm");
  var confirmBody = document.getElementById("confirm-body");
  var qrDlg = document.getElementById("qr");
  var qrFrame = document.getElementById("qr-frame");
  var qrTitle = document.getElementById("qr-title");
  var qrDownload = document.getElementById("qr-download");

  var links = [];
  var cursor = null;
  var pending = null;

  /* ── words for failures ───────────────────────────────────────────────── */

  /* The server's message is server-authored and safe to show, but it is
     written for a client and not for a person. These are the cases worth
     saying differently. */
  function explain(status, body) {
    if (status === 409) {
      return "That code is already taken. Pick another, or repoint the link you have.";
    }
    if (status === 429) {
      return "That is a lot of requests in a short time. Wait a few seconds and try again.";
    }
    if (status === 401) {
      return "You have been signed out. Reload the page and sign in again.";
    }
    if (status === 502 || status === 503 || status === 504) {
      return "Your repository did not answer. This is usually brief — try again in a moment.";
    }
    if (body && body.error) return body.error;
    return "That did not work. Try again.";
  }

  function api(method, path, payload) {
    var init = { method: method, headers: { accept: "application/json" } };
    if (payload !== undefined) {
      init.headers["content-type"] = "application/json";
      init.body = JSON.stringify(payload);
    }
    return fetch(path, init).then(function (res) {
      if (res.status === 204) return null;
      return res
        .json()
        .catch(function () {
          return null;
        })
        .then(function (body) {
          if (!res.ok) {
            var err = new Error(explain(res.status, body));
            err.status = res.status;
            throw err;
          }
          return body;
        });
    });
  }

  function show(el, on) {
    if (el) el.hidden = !on;
  }

  function say(el, text) {
    if (!el) return;
    el.textContent = text;
    el.hidden = !text;
  }

  /* ── rendering ────────────────────────────────────────────────────────── */

  function shortUrlFor(code) {
    return location.origin + location.pathname.replace(/\/dashboard\/?$/, "") + "/@" + handleOf() + "/" + code;
  }

  var cachedHandle = null;
  function handleOf() {
    if (cachedHandle) return cachedHandle;
    var el = document.querySelector(".whoami-handle");
    cachedHandle = el ? el.textContent.replace(/^@/, "").trim() : "";
    return cachedHandle;
  }

  /* Relative where it helps, exact in the title and in `datetime`. The old UI
     had a local/UTC toggle, which was a control for a problem the design
     should not have. */
  function when(iso) {
    if (!iso) return { text: "", exact: "" };
    var then = new Date(iso);
    if (isNaN(then)) return { text: iso, exact: iso };
    var secs = (Date.now() - then.getTime()) / 1000;
    var text;
    if (secs < 90) text = "just now";
    else if (secs < 3600) text = Math.round(secs / 60) + " min ago";
    else if (secs < 86400) text = Math.round(secs / 3600) + " hr ago";
    else if (secs < 2592000) text = Math.round(secs / 86400) + " days ago";
    else text = then.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
    return { text: text, exact: then.toLocaleString() };
  }

  function row(link) {
    var li = document.createElement("li");
    li.className = "rackrow";
    li.dataset.code = link.code;

    var cell = document.createElement("span");
    cell.className = "cellsq on rackrow-cell";
    li.appendChild(cell);

    var body = document.createElement("div");
    body.className = "rackrow-body";

    var code = document.createElement("a");
    code.className = "rackrow-code";
    code.href = shortUrlFor(link.code);
    code.textContent = link.code;
    body.appendChild(code);

    var dest = document.createElement("a");
    dest.className = "rackrow-dest";
    dest.href = link.url;
    dest.textContent = link.url;
    dest.rel = "noopener nofollow";
    body.appendChild(dest);

    li.appendChild(body);

    var stamp = when(link.updated_at);
    var time = document.createElement("time");
    time.className = "rackrow-when num";
    if (link.updated_at) {
      time.dateTime = link.updated_at;
      time.title = stamp.exact;
    }
    time.textContent = stamp.text;
    li.appendChild(time);

    var acts = document.createElement("div");
    acts.className = "rackrow-acts";
    acts.appendChild(iconBtn("Copy", "copy", link));
    acts.appendChild(iconBtn("Repoint", "edit", link));
    acts.appendChild(iconBtn("QR code", "qr", link));
    acts.appendChild(iconBtn("Delete", "delete", link, true));
    li.appendChild(acts);

    return li;
  }

  var MARK =
    '<svg class="mark" viewBox="0 0 15 15" aria-hidden="true" focusable="false"><g fill="currentColor">' +
    '<rect x="1" y="1" width="5" height="5"/><rect x="9" y="1" width="5" height="5"/>' +
    '<rect x="1" y="9" width="5" height="5"/><rect x="9" y="9" width="5" height="5"/></g></svg>';

  function iconBtn(label, action, link, danger) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "icobtn" + (danger ? " icobtn--danger" : "");
    b.title = label;
    b.setAttribute("aria-label", label + " " + link.code);
    b.dataset.action = action;
    b.innerHTML = MARK;
    return b;
  }

  function draw() {
    var q = (filter.value || "").trim().toLowerCase();
    var hits = q
      ? links.filter(function (l) {
          return (l.code + " " + l.url).toLowerCase().indexOf(q) !== -1;
        })
      : links;

    rack.innerHTML = "";
    hits.forEach(function (l) {
      rack.appendChild(row(l));
    });

    show(empty, links.length === 0);
    say(nomatch, links.length && !hits.length ? 'Nothing here matches "' + q + '".' : "");
    count.textContent = links.length ? links.length + (cursor ? "+" : "") : "";
    show(moreWrap, Boolean(cursor));
  }

  /* ── loading ──────────────────────────────────────────────────────────── */

  function load(more) {
    var path = "/api/links?limit=" + PAGE + (more && cursor ? "&cursor=" + encodeURIComponent(cursor) : "");
    show(loading, !more && links.length === 0);
    say(rackError, "");

    return api("GET", path)
      .then(function (body) {
        show(loading, false);
        var page = (body && body.links) || [];
        links = more ? links.concat(page) : page;
        cursor = (body && body.cursor) || null;
        draw();
      })
      .catch(function (err) {
        show(loading, false);
        say(rackError, err.message);
      });
  }

  /* ── writing ──────────────────────────────────────────────────────────── */

  makeForm.addEventListener("submit", function (e) {
    e.preventDefault();
    say(makeError, "");

    var payload = { url: urlInput.value.trim() };
    var code = codeInput.value.trim();
    if (code) payload.code = code;

    var btn = makeForm.querySelector(".make-go");
    btn.disabled = true;

    api("POST", "/api/shorten", payload)
      .then(function (body) {
        urlInput.value = "";
        codeInput.value = "";
        return copy(body.short_url).then(function (copied) {
          return load(false).then(function () {
            var made = rack.querySelector('[data-code="' + cssEscape(lastSegment(body.short_url)) + '"]');
            if (made) {
              var cell = made.querySelector(".rackrow-cell");
              if (cell) {
                cell.classList.add("strike");
                setTimeout(function () {
                  cell.classList.remove("strike");
                }, 700);
              }
              flash(made, copied ? "Written, and copied" : "Written");
            }
          });
        });
      })
      .catch(function (err) {
        say(makeError, err.message);
        if (err.status === 409) codeInput.focus();
      })
      .finally(function () {
        btn.disabled = false;
      });
  });

  function lastSegment(url) {
    return String(url).split("/").pop();
  }

  function cssEscape(s) {
    return String(s).replace(/["\\]/g, "\\$&");
  }

  function copy(text) {
    if (!navigator.clipboard) return Promise.resolve(false);
    return navigator.clipboard
      .writeText(text)
      .then(function () {
        return true;
      })
      .catch(function () {
        return false;
      });
  }

  function flash(rowEl, text) {
    var note = rowEl.querySelector(".rackrow-flash");
    if (!note) {
      note = document.createElement("span");
      note.className = "rackrow-flash lbl";
      rowEl.appendChild(note);
    }
    note.textContent = text;
    note.hidden = false;
    clearTimeout(note._t);
    note._t = setTimeout(function () {
      note.hidden = true;
    }, 2200);
  }

  /* ── row actions ──────────────────────────────────────────────────────── */

  rack.addEventListener("click", function (e) {
    var btn = e.target.closest("[data-action]");
    if (!btn) return;
    var rowEl = btn.closest(".rackrow");
    var code = rowEl.dataset.code;
    var link = links.find(function (l) {
      return l.code === code;
    });
    if (!link) return;

    if (btn.dataset.action === "copy") {
      copy(shortUrlFor(code)).then(function (ok) {
        flash(rowEl, ok ? "Copied" : "Press ⌘C to copy");
      });
    } else if (btn.dataset.action === "delete") {
      askDelete(link, rowEl);
    } else if (btn.dataset.action === "qr") {
      openQr(link);
    } else if (btn.dataset.action === "edit") {
      openEdit(link, rowEl);
    }
  });

  /* Repointing is the interaction that makes ownership concrete, so it happens
     in place rather than behind a dialog: the row becomes editable and the
     destination is right there to change. */
  function openEdit(link, rowEl) {
    if (rowEl.querySelector(".rackrow-edit")) return;

    var form = document.createElement("form");
    form.className = "rackrow-edit";

    var field = document.createElement("div");
    field.className = "field";
    var input = document.createElement("input");
    input.type = "url";
    input.required = true;
    input.value = link.url;
    input.setAttribute("aria-label", "New destination for " + link.code);
    field.appendChild(input);
    form.appendChild(field);

    var save = document.createElement("button");
    save.type = "submit";
    save.className = "btn btn--fill";
    save.textContent = "Repoint";
    form.appendChild(save);

    var cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "btn";
    cancel.textContent = "Cancel";
    cancel.addEventListener("click", function () {
      form.remove();
    });
    form.appendChild(cancel);

    var err = document.createElement("p");
    err.className = "state state--err rackrow-edit-error";
    err.hidden = true;
    form.appendChild(err);

    form.addEventListener("submit", function (e) {
      e.preventDefault();
      save.disabled = true;
      err.hidden = true;
      api("PUT", "/api/shorten/" + encodeURIComponent(link.code), { url: input.value.trim() })
        .then(function () {
          link.url = input.value.trim();
          form.remove();
          draw();
          var again = rack.querySelector('[data-code="' + cssEscape(link.code) + '"]');
          if (again) flash(again, "Repointed");
        })
        .catch(function (e2) {
          err.textContent = e2.message;
          err.hidden = false;
          save.disabled = false;
        });
    });

    rowEl.appendChild(form);
    input.focus();
    input.select();
  }

  function askDelete(link, rowEl) {
    // Name the code. "Are you sure?" asks people to remember what they clicked.
    confirmBody.textContent =
      "Deleting " + link.code + " removes the record from your repository. " +
      "Anyone who follows the link after that gets a 404.";
    confirmDlg.returnValue = "";
    confirmDlg.showModal();

    confirmDlg.addEventListener(
      "close",
      function () {
        if (confirmDlg.returnValue !== "delete") return;
        api("DELETE", "/api/shorten/" + encodeURIComponent(link.code))
          .then(function () {
            links = links.filter(function (l) {
              return l.code !== link.code;
            });
            draw();
          })
          .catch(function (err) {
            say(rackError, err.message);
          });
      },
      { once: true }
    );
  }

  function openQr(link) {
    var url = "/@" + handleOf() + "/" + encodeURIComponent(link.code) + "/qr";
    qrTitle.textContent = "QR code for " + link.code;
    qrFrame.innerHTML = "";
    var img = document.createElement("img");
    img.src = url;
    img.alt = "QR code for " + shortUrlFor(link.code);
    qrFrame.appendChild(img);
    qrDownload.href = url;
    qrDownload.setAttribute("download", link.code + ".svg");
    qrDlg.showModal();
  }

  document.getElementById("qr-close").addEventListener("click", function () {
    qrDlg.close();
  });

  filter.addEventListener("input", draw);
  moreBtn.addEventListener("click", function () {
    moreBtn.disabled = true;
    load(true).finally(function () {
      moreBtn.disabled = false;
    });
  });

  load(false);
})();
