/* The live wall: to.atpr.link records as they are written across the network.
 *
 * This is the page's proof. Not a claim that people use this, but the records
 * themselves arriving, with the handle of whoever wrote each one.
 *
 * The version removed in the rebrand had no reconnect and no error or close
 * handling at all, so the first dropped socket ended the feed silently for the
 * rest of the session. This one backs off and comes back.
 *
 * Everything here is decoration over a page that is complete without it. If
 * Jetstream is unreachable, the quiet state stands and nothing else on the
 * page is affected.
 */
(function () {
  "use strict";

  var HOSTS = [
    "wss://jetstream1.us-east.bsky.network/subscribe",
    "wss://jetstream2.us-east.bsky.network/subscribe",
  ];
  var COLLECTION = "to.atpr.link";
  var PLC = "https://plc.directory/";

  var MAX_ITEMS = 6;
  var BACKOFF_MIN_MS = 1000;
  var BACKOFF_MAX_MS = 30000;
  /* Long enough that a genuinely slow evening does not look like a fault, and
     short enough that a dead socket does not sit there looking alive. */
  var QUIET_AFTER_MS = 90000;

  var list = document.getElementById("wall");
  var quiet = document.getElementById("wall-quiet");
  if (!list || !quiet) return;

  var socket = null;
  var host = 0;
  var backoff = BACKOFF_MIN_MS;
  var quietTimer = null;
  var handles = new Map();
  var seen = new Set();
  /* Bumped whenever a socket is abandoned. Handlers compare against it rather
     than being removed, which is the only reliable way to ignore a late event
     from a connection we have already given up on. */
  var generation = 0;
  var paused = false;

  function showQuiet(show) {
    quiet.hidden = !show;
  }

  function markBusy() {
    showQuiet(false);
    clearTimeout(quietTimer);
    quietTimer = setTimeout(function () {
      if (!list.children.length) showQuiet(true);
    }, QUIET_AFTER_MS);
  }

  /* DID -> handle. Failures resolve to null rather than rejecting: an entry
     with no handle is still worth showing, and one unresolvable DID must not
     take down the row it belongs to. */
  function handleFor(did) {
    if (handles.has(did)) return Promise.resolve(handles.get(did));
    return fetch(PLC + encodeURIComponent(did))
      .then(function (r) {
        return r.ok ? r.json() : null;
      })
      .then(function (doc) {
        var found = null;
        if (doc && Array.isArray(doc.alsoKnownAs)) {
          for (var i = 0; i < doc.alsoKnownAs.length; i++) {
            if (doc.alsoKnownAs[i].indexOf("at://") === 0) {
              found = doc.alsoKnownAs[i].slice(5);
              break;
            }
          }
        }
        handles.set(did, found);
        return found;
      })
      .catch(function () {
        handles.set(did, null);
        return null;
      });
  }

  function add(did, code) {
    var key = did + "/" + code;
    if (seen.has(key)) return;
    seen.add(key);

    var li = document.createElement("li");
    li.className = "wall-item";

    var cell = document.createElement("span");
    // The strike: the same motion the dashboard uses when you write your own.
    cell.className = "cellsq on strike";
    li.appendChild(cell);

    var text = document.createElement("span");
    text.className = "wall-text";

    var who = document.createElement("span");
    who.className = "wall-who";
    who.textContent = "…";
    text.appendChild(who);

    var codeEl = document.createElement("span");
    codeEl.className = "wall-code";
    codeEl.textContent = code;
    text.appendChild(codeEl);

    li.appendChild(text);
    list.insertBefore(li, list.firstChild);

    while (list.children.length > MAX_ITEMS) {
      list.removeChild(list.lastChild);
    }
    markBusy();

    handleFor(did).then(function (handle) {
      who.textContent = handle ? "@" + handle : did.slice(0, 20) + "…";
    });
  }

  function connect() {
    var url =
      HOSTS[host % HOSTS.length] +
      "?wantedCollections=" +
      encodeURIComponent(COLLECTION);

    try {
      socket = new WebSocket(url);
    } catch (e) {
      return retry();
    }

    /* Every handler is bound to the generation that created it. A socket that
       errors and *then* closes, or that closes long after it was abandoned,
       fires into a stale generation and is ignored. Nulling `onclose` would
       not help here: these are addEventListener handlers, so the property
       assignment removes nothing. */
    var mine = generation;
    var live = function () {
      return mine === generation;
    };

    socket.addEventListener("open", function () {
      if (!live()) return;
      backoff = BACKOFF_MIN_MS;
      markBusy();
    });

    socket.addEventListener("message", function (event) {
      if (!live()) return;
      var msg;
      try {
        msg = JSON.parse(event.data);
      } catch (e) {
        return;
      }
      var c = msg && msg.commit;
      if (!c || !msg.did) return;
      if (c.collection !== COLLECTION) return;
      if (c.operation !== "create" && c.operation !== "update") return;
      if (!c.rkey) return;
      add(msg.did, c.rkey);
    });

    // Either can fire, and both can fire in sequence for one failure.
    socket.addEventListener("error", function () {
      if (live()) retry();
    });
    socket.addEventListener("close", function () {
      if (live()) retry();
    });
  }

  function drop() {
    // Retiring the generation is what actually detaches the old socket: every
    // handler on it checks its own generation before doing anything.
    generation += 1;
    if (socket) {
      try {
        socket.close();
      } catch (e) {
        /* already closing */
      }
      socket = null;
    }
  }

  function retry() {
    drop();
    if (!list.children.length) showQuiet(true);
    if (paused) return;

    // Alternate hosts, so one unhealthy instance is not retried forever.
    host += 1;
    var scheduled = generation;
    setTimeout(function () {
      // A pause or a further failure during the wait retires this attempt.
      if (paused || scheduled !== generation) return;
      connect();
    }, backoff);
    backoff = Math.min(backoff * 2, BACKOFF_MAX_MS);
  }

  /* Nothing to watch while the tab is hidden, and a socket held open across a
     long background stint is the one most likely to come back dead. Pausing
     has to suppress the reconnect too: closing the socket fires `close`, and
     without the flag that would schedule a reconnect for a tab nobody is
     looking at. */
  document.addEventListener("visibilitychange", function () {
    if (document.hidden) {
      paused = true;
      drop();
      clearTimeout(quietTimer);
    } else if (paused) {
      paused = false;
      backoff = BACKOFF_MIN_MS;
      connect();
    }
  });

  connect();
})();
