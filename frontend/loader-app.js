// Loader of an app page (scripts/build-web.sh writes the file names; a file, not an inline script: CSP). It starts
// the .wasm of the page's group of screens, then prepares the whole app.
import init, { altimStop } from "/{{GLUE_JS}}";

init({ module_or_path: "/{{WASM}}" });

// The whole app (every screen), fetched and compiled in the background once this page has loaded: a move to another
// group's screen then shows in place (window.altimFull, called by the .wasm, see altim_web::part) instead of loading
// a page. Not when the browser asks to save data.
const FULL_AFTER_MS = 2000;
addEventListener("load", () => {
  if (navigator.connection && navigator.connection.saveData) return;
  setTimeout(() => {
    Promise.all([import("/{{FULL_GLUE}}"), WebAssembly.compileStreaming(fetch("/{{FULL_WASM}}"))]).then(
      ([full, module]) => {
        window.altimFull = () => {
          delete window.altimFull;
          setTimeout(() => {
            // The page keeps its last picture until the whole app draws over it.
            const root = document.getElementById("root");
            const picture = root.innerHTML;
            altimStop();
            root.innerHTML = picture;
            full.default({ module_or_path: module });
          });
        };
      },
      () => {},
    );
  }, FULL_AFTER_MS);
});
