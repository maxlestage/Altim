// Before the app's frame drawn in an app page (frontend/shell.html, shown until the .wasm draws the screen): hidden
// when this browser has not accepted the disclaimer yet ("altim.webapp.v1", read like AppState::parse), which the app
// shows instead. A file, not an inline script (CSP); it runs before the frame is parsed.
try {
  const s = JSON.parse(localStorage.getItem("altim.webapp.v1"));
  if (!(s && s.version === 1 && s.acceptedDisclaimer)) document.documentElement.classList.add("no-shell");
} catch (e) {
  document.documentElement.classList.add("no-shell");
}
// Just signed in (the page comes from /login): the Altim logo is traced once over the frame, gone in 0.9 s, never in
// the way (no pointer events; the screen draws underneath meanwhile). Not with reduced motion.
try {
  const from = document.referrer && new URL(document.referrer);
  if (from && from.origin === location.origin && from.pathname === "/login" && !matchMedia("(prefers-reduced-motion: reduce)").matches) {
    document.currentScript.insertAdjacentHTML(
      "afterend",
      '<div class="arrive" aria-hidden="true"><svg viewBox="0 0 1024 1024"><path pathLength="1" d="M232 780 L512 214 L792 780"/>' +
        '<path pathLength="1" class="acc" d="M330 600 L430 520 L520 575 L700 420"/></svg></div>',
    );
    setTimeout(() => document.querySelector(".arrive")?.remove(), 1200);
  }
} catch (e) {}
