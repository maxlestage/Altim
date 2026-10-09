// The opening of the home page (« la porte »), once per browser session: the Altim logo traced stroke by stroke on a
// dark screen, a counter of what has really loaded, then the door opens onto the page. scripts/build-web.sh puts this
// file at the head of the site's loader (frontend/loader-site.js; a module, so it never holds the parsing up), which
// runs it before the page is drawn. The door's markup (frontend/door.html) stays hidden unless this script shows it:
// a visitor who has seen it this session, or asked for reduced motion, never sees it.
//
// The counter only moves on real events, weighted: the site's .wasm, by bytes received (reported by
// frontend/loader-site.js, 80 %), the fonts of the hero (10 %), the first answer of the server's data (10 %). The door
// opens once the page is drawn (loader-site.js: `altimDoor.mounted()`): when the logo is traced and everything has
// loaded, and never later than 0.7 s after the page is drawn, whatever the counter says. A tap or a key opens it at
// once; it opens anyway after 12 s (the page failed to load: no trap).
(function () {
  var KEY = "altim.door";
  var html = document.documentElement;
  try {
    if (location.pathname !== "/" || sessionStorage.getItem(KEY) || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    sessionStorage.setItem(KEY, "1");
  } catch (e) {
    return;
  }
  html.classList.add("door-on");
  var t0 = performance.now();
  var TRACE = 1150;
  var WAIT = 700;
  var weights = { wasm: 0.8, fonts: 0.1, data: 0.1 };
  var got = { wasm: 0, fonts: 0, data: 0 };
  var shown = 0;
  var mountedAt = 0;
  var doneAt = 0;
  var opened = false;
  var value = function () {
    var v = 0;
    for (var k in weights) v += weights[k] * got[k];
    return Math.min(1, v);
  };
  var paint = function () {
    var el = document.getElementById("door-n");
    if (el) el.textContent = String(Math.floor(shown * 100 + 1e-6));
  };
  var open = function () {
    if (opened) return;
    opened = true;
    shown = value();
    paint();
    html.classList.add("door-open");
    setTimeout(function () {
      html.classList.remove("door-on", "door-open");
      var d = document.getElementById("door");
      if (d) d.remove();
    }, 800);
  };
  var tick = function () {
    if (opened) return;
    var now = performance.now();
    // The counter climbs towards what has really loaded, never past it.
    var v = value();
    shown = Math.min(v, shown + Math.max(0.01, (v - shown) * 0.2));
    paint();
    if (v >= 1 && !doneAt) doneAt = now;
    if (mountedAt && ((doneAt && now - t0 >= TRACE) || now - mountedAt >= WAIT)) return open();
    if (now - t0 > 12000) return open();
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
  window.altimDoor = {
    set: function (part, fraction) {
      if (part in got) got[part] = Math.max(got[part], Math.min(1, fraction));
    },
    mounted: function () {
      if (!mountedAt) mountedAt = performance.now();
    },
    open: open,
  };
  // The hero's fonts (Google Fonts, display=swap): loaded or failed, the step is over.
  var fonts = function () {
    got.fonts = 1;
  };
  try {
    Promise.all([document.fonts.load("900 1em Orbitron"), document.fonts.load("400 1em 'Space Grotesk'")]).then(fonts, fonts);
  } catch (e) {
    fonts();
  }
  // The first answer of the server's data (prices of the hero): the first /api/ resource that completes.
  try {
    new PerformanceObserver(function (list) {
      list.getEntries().forEach(function (e) {
        if (e.name.indexOf("/api/") >= 0) got.data = 1;
      });
    }).observe({ type: "resource", buffered: true });
  } catch (e) {
    got.data = 1;
  }
  var skip = function () {
    if (html.classList.contains("door-on")) open();
  };
  addEventListener("pointerdown", skip, { passive: true });
  addEventListener("keydown", skip);
})();
