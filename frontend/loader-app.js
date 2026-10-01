// Loader of an app page (scripts/build-web.sh writes the file names; a file, not an inline script: CSP). It starts
// the .wasm of the page's group of screens, then prepares the others so that moving to them needs no page load.
import init, { altimStop, altimGroupOf } from "/{{GLUE_JS}}";

// Every part's glue and .wasm ("app" is the whole app), and the groups to prepare as soon as the page is shown
// (a group whose screens sit one tap away, e.g. Mes avoirs ⇄ Simulation).
const PARTS = {{PARTS}};
const NEIGHBOURS = {{NEIGHBOURS}};
const OWN = "{{GROUP}}";

init({ module_or_path: "/{{WASM}}" }).then(() => NEIGHBOURS.forEach(load), () => {});

const compiled = {};
const ready = {};
// A glue already started in this page cannot start again (wasm-bindgen keeps one instance per glue).
const used = new Set([OWN]);
let stop = altimStop;
let groupOf = altimGroupOf;

/// Fetches and compiles a part once (the glue imported, the .wasm compiled, not started).
function load(name) {
  if (compiled[name] || used.has(name) || !PARTS[name]) return;
  const [glue, wasm] = PARTS[name];
  compiled[name] = Promise.all([import(glue), WebAssembly.compileStreaming(fetch(wasm))]).then(
    (r) => (ready[name] = r),
    () => delete compiled[name],
  );
}

/// The part that can show the group `g` in place: the whole app when ready, else that group's own .wasm.
function target(g) {
  if (ready.app && !used.has("app")) return "app";
  if (g && g !== "site" && ready[g] && !used.has(g)) return g;
  return null;
}

// Called by the .wasm (altim_web::part::hand_over): `altimFull.ready(g)` says whether the group's address can show
// in place, `altimFull(g)` hands the page over: the current .wasm stops, the page keeps its last picture until the
// new one draws over it.
window.altimFull = (g) => {
  const name = target(g);
  if (!name) return false;
  used.add(name);
  const [glue, module] = ready[name];
  setTimeout(() => {
    const root = document.getElementById("root");
    const picture = root.innerHTML;
    stop();
    root.innerHTML = picture;
    // Its exports only once it has started (before that its glue has no instance).
    stop = () => {};
    groupOf = () => "";
    glue.default({ module_or_path: module }).then(() => {
      stop = glue.altimStop;
      groupOf = glue.altimGroupOf;
    });
  });
  return true;
};
window.altimFull.ready = (g) => target(g) !== null;

// The target's group prepared when a link is about to be followed (pointer over it, finger down, keyboard focus).
const intent = (e) => {
  const a = e.target instanceof Element && e.target.closest("a[href]");
  if (!a || ready.app) return;
  const g = groupOf(a.getAttribute("href"));
  if (g && g !== "site") load(g);
};
for (const type of ["pointerover", "touchstart", "focusin"]) addEventListener(type, intent, { passive: true });

// The whole app (every screen), fetched and compiled in the background once this page has loaded: from then on
// every move shows in place. Not when the browser asks to save data.
const FULL_AFTER_MS = 2000;
addEventListener("load", () => {
  if (navigator.connection && navigator.connection.saveData) return;
  setTimeout(() => load("app"), FULL_AFTER_MS);
});
