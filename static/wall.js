/* The live wall: to.atpr.link records as they are written across the network.
 *
 * This is the page's proof. Not a claim that people use this, but the records
 * themselves arriving, with the handle of whoever wrote each one.
 *
 * Everything here is decoration over a page that is complete without it. If
 * Jetstream is unreachable, the quiet state stands and nothing else on the
 * page is affected. The socket itself -- reconnect, backoff, tab-hide -- lives
 * in jetstream.js, because the dashboard needs the same thing and the awkward
 * parts of it were not worth writing twice.
 */
(function () {
  "use strict";

  var PLC = "https://plc.directory/";
  var MAX_ITEMS = 6;

  var list = document.getElementById("wall");
  var quiet = document.getElementById("wall-quiet");
  if (!list || !quiet || !window.atprJetstream) return;

  var handles = new Map();
  var seen = new Set();

  /* The empty state is a function of the list being empty, and of nothing
     else. It used to be cleared when the socket opened, which meant the
     explanation of what this panel is for vanished the moment the connection
     succeeded and left a blank rectangle behind it -- on a low-traffic product
     that is the state most visitors see, and connecting is not news to
     anybody. It goes when a record lands, and not before. */
  function settle() {
    quiet.hidden = list.children.length > 0;
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

  /* What each operation is called, and how its cell behaves.
   *
   * A create fills a cell and strikes it, which is the motion the dashboard
   * uses when you write your own. An update strikes an already-filled cell,
   * because the record was there before and still is. A delete leaves the cell
   * pierced open -- the world's own duality doing the work, with no second
   * colour and no icon.
   *
   * Oxide red is *not* used for a delete. It means destruction the visitor is
   * doing or has suffered; a stranger removing their own link is neither, and
   * spending the one chromatic value on other people's routine housekeeping
   * would empty it. */
  var OPERATIONS = {
    create: { verb: "wrote a link", cell: "cellsq on strike" },
    update: { verb: "repointed link", cell: "cellsq on strike" },
    delete: { verb: "deleted link", cell: "cellsq" },
  };

  /* Who, and what they did. Deliberately not *what the link is*.
   *
   * The short code and the destination are both written by a stranger and are
   * shown to everyone who loads the front page. That is an open invitation:
   * a code reading `paypal-verify`, a destination on a domain that only has to
   * look plausible for the length of a glance. Escaping them is not the
   * problem -- `textContent` handles that -- the problem is that publishing an
   * unreviewed stranger's chosen words on our homepage lends them this site's
   * credibility, and no amount of escaping fixes that.
   *
   * A handle is not the same risk: it is a domain somebody had to register or
   * a DID they had to control, and it is the fact this panel is actually
   * demonstrating -- that real accounts across the network are writing these
   * records. The verb is ours.
   *
   * `code` still identifies the event for de-duplication. It is never rendered. */
  function add(did, code, operation) {
    var op = OPERATIONS[operation];
    if (!op) return;

    // A link repointed twice is two events; the same event arriving twice is not.
    var key = did + "/" + code + "/" + operation;
    if (seen.has(key)) return;
    seen.add(key);

    var li = document.createElement("li");
    li.className = "wall-item";

    var cell = document.createElement("span");
    cell.className = op.cell;
    li.appendChild(cell);

    var text = document.createElement("span");
    text.className = "wall-text";

    var who = document.createElement("span");
    who.className = "wall-who";
    who.textContent = "…";
    text.appendChild(who);

    var verb = document.createElement("span");
    verb.className = "wall-verb lbl";
    verb.textContent = op.verb;
    text.appendChild(verb);

    li.appendChild(text);
    list.insertBefore(li, list.firstChild);

    while (list.children.length > MAX_ITEMS) {
      list.removeChild(list.lastChild);
    }
    settle();

    handleFor(did).then(function (handle) {
      who.textContent = handle ? "@" + handle : did.slice(0, 20) + "…";
    });
  }

  // Everyone's records, which is the point of this panel.
  window.atprJetstream.subscribe({
    collection: window.atprJetstream.COLLECTION,
    onCommit: add,
    onDrop: settle,
  });
})();
