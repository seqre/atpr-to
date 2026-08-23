/* A Jetstream subscription that survives the network.
 *
 * Two surfaces need this now — the landing page's wall, watching everybody, and
 * the dashboard, watching one repo — and the awkward parts are the same for
 * both: a socket that errors *and* closes for one failure, a reconnect that
 * must not fire twice, and a tab that goes to the background for an hour and
 * comes back holding a socket that is open in name only.
 *
 * The first version of this lived in wall.js and had two bugs that are easy to
 * write and hard to see, so it is here once rather than in two files:
 *
 *   - `socket.onclose = null` removes nothing when the handler was added with
 *     addEventListener, so a dropped socket scheduled two reconnects.
 *   - pausing on tab-hide closed the socket, which fired `close`, which queued
 *     a reconnect for a tab nobody was looking at.
 *
 * Both are fixed by a generation counter: handlers compare against it instead
 * of being removed, and anything from a retired socket is ignored.
 *
 * Exposes one global on purpose. There is no bundler here and CSP forbids
 * inline script, so this is a plain classic script that the two consumers load
 * before their own.
 */
(function (global) {
  "use strict";

  var HOSTS = [
    "wss://jetstream1.us-east.bsky.network/subscribe",
    "wss://jetstream2.us-east.bsky.network/subscribe",
  ];
  var BACKOFF_MIN_MS = 1000;
  var BACKOFF_MAX_MS = 30000;

  /* Subscribe to one collection, optionally narrowed to one repo.
   *
   * `options.collection`  required; the lexicon id to filter on.
   * `options.did`         optional; when given, only that repo's commits.
   * `options.onCommit`    called with (did, rkey, operation, record).
   * `options.onOpen`      optional; called on every successful connection.
   *
   * Returns nothing. A subscription lives as long as the page does — there is
   * no caller here that needs to stop one, and an unused `close()` is a method
   * that will be wrong by the time somebody needs it.
   */
  function subscribe(options) {
    var socket = null;
    var host = 0;
    var backoff = BACKOFF_MIN_MS;
    var generation = 0;
    var paused = false;

    function url() {
      var u =
        HOSTS[host % HOSTS.length] +
        "?wantedCollections=" +
        encodeURIComponent(options.collection);
      // Narrowing to a repo is what makes the dashboard's subscription cheap:
      // the server does the filtering, so the tab is not woken for every
      // record written anywhere on the network.
      if (options.did) u += "&wantedDids=" + encodeURIComponent(options.did);
      return u;
    }

    function connect() {
      try {
        socket = new WebSocket(url());
      } catch (e) {
        return retry();
      }

      var mine = generation;
      var live = function () {
        return mine === generation;
      };

      socket.addEventListener("open", function () {
        if (!live()) return;
        backoff = BACKOFF_MIN_MS;
        if (options.onOpen) options.onOpen();
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
        if (c.collection !== options.collection) return;
        if (!c.rkey) return;
        // A delete carries no record, which is exactly what makes it a delete.
        options.onCommit(msg.did, c.rkey, c.operation, c.record);
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
      // Retiring the generation is what actually detaches the old socket.
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
      if (options.onDrop) options.onDrop();
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
       long background stint is the one most likely to come back dead. */
    document.addEventListener("visibilitychange", function () {
      if (document.hidden) {
        paused = true;
        drop();
      } else if (paused) {
        paused = false;
        backoff = BACKOFF_MIN_MS;
        connect();
      }
    });

    connect();
  }

  global.atprJetstream = { subscribe: subscribe, COLLECTION: "to.atpr.link" };
})(window);
