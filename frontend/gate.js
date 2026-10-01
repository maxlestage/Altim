// Before the app's frame drawn in an app page (frontend/shell.html, shown until the .wasm draws the screen): hidden
// when this browser has not accepted the disclaimer yet ("altim.webapp.v1", read like AppState::parse), which the app
// shows instead. A file, not an inline script (CSP); it runs before the frame is parsed.
try {
  const s = JSON.parse(localStorage.getItem("altim.webapp.v1"));
  if (!(s && s.version === 1 && s.acceptedDisclaimer)) document.documentElement.classList.add("no-shell");
} catch (e) {
  document.documentElement.classList.add("no-shell");
}
