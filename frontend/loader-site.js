// Loader of the presentation site's pages (scripts/build-web.sh writes the file names and puts frontend/door.js
// before it; a file, not an inline script: CSP). It starts the site's .wasm; while the home page's opening is shown
// (door.js), it reads the .wasm as it arrives to report the real bytes received (its decoded size is known at build
// time), then tells the opening that the page is drawn. After the first paint, it loads the site's motion
// (frontend/motion.js).
import init from "/{{GLUE_JS}}";

const WASM = "/{{WASM}}";
const SIZE = {{WASM_SIZE}};
const door = window.altimDoor;

/// The .wasm, counted byte by byte for the opening (one copy of the stream compiles while the other is counted).
function wasm() {
  if (!door || typeof ReadableStream !== "function") return WASM;
  return fetch(WASM).then((res) => {
    if (!res.ok || !res.body || !res.body.tee) return res;
    const [compile, count] = res.body.tee();
    const reader = count.getReader();
    let got = 0;
    const pump = () =>
      reader.read().then(({ done, value }) => {
        if (done) return door.set("wasm", 1);
        got += value.byteLength;
        door.set("wasm", got / SIZE);
        return pump();
      });
    pump().catch(() => {});
    return new Response(compile, { headers: { "content-type": "application/wasm" } });
  });
}

init({ module_or_path: wasm() }).then(
  () => {
    // Drawn on the next frame: the opening may open.
    requestAnimationFrame(() => door && door.mounted());
    const later = () => import("/{{MOTION_JS}}").catch(() => {});
    const idle = (f) => (window.requestIdleCallback ? requestIdleCallback(f, { timeout: 2000 }) : setTimeout(f, 300));
    if (document.readyState === "complete") idle(later);
    else addEventListener("load", () => idle(later), { once: true });
  },
  () => door && door.open(),
);
