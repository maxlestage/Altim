// Motion of the presentation site, loaded by the site's loader after the first paint (frontend/loader-site.js; a
// file, not an inline script: CSP). Nothing here holds content: every caption is text drawn by the site's .wasm.
// - The home page's scroll « film » (altim_web::site::film): thousands of luminous grains form one Altim line drawing
//   per station (logo, candles, radar, balance, shield, devices, logo) and morph from one to the next as the page
//   scrolls. WebGL: the shapes are uploaded once, a frame only sets a few uniforms (the morph runs in the vertex
//   shader); Canvas 2D with fewer grains when WebGL is missing. Paused off screen and in a hidden tab.
// - Reduced motion: no film; one still drawing per caption.
// - Fine pointers only (hover: hover and pointer: fine): a soft round cursor and magnetic buttons. Touch scrolling is
//   never touched, and the page's scroll stays native everywhere.

const BROWSER = typeof document === "object" && typeof matchMedia === "function";
const REDUCED = BROWSER && matchMedia("(prefers-reduced-motion: reduce)").matches;
const FINE = BROWSER && matchMedia("(hover: hover) and (pointer: fine)").matches;
const WHITE = [0.93, 0.96, 1.0];
const CYAN = [0.0, 0.94, 1.0];

// ---------- Shapes: Altim line drawings in a box of about [-1, 1]², y down ----------

const line = (pts, a = 0, w = 1) => ({ pts, a, w });
const loop = (pts, a, w) => line([...pts, pts[0], pts[1]], a, w);
const rect = (x, y, w, h, a, wt) => loop([x, y, x + w, y, x + w, y + h, x, y + h], a, wt);
function arc(cx, cy, r, a0, a1, a = 0, w = 1, n = 40) {
  const pts = [];
  for (let i = 0; i <= n; i++) {
    const t = a0 + ((a1 - a0) * i) / n;
    pts.push(cx + r * Math.cos(t), cy + r * Math.sin(t));
  }
  return line(pts, a, w);
}
const circle = (cx, cy, r, a, w) => arc(cx, cy, r, 0, Math.PI * 2, a, w, 64);
const disc = (cx, cy, r, a = 1, w = 1) => ({ disc: [cx, cy, r], a, w });
function rrect(x, y, w, h, r, a, wt) {
  const q = Math.PI / 2;
  const parts = [arc(x + w - r, y + r, r, -q, 0, a, wt, 8), arc(x + w - r, y + h - r, r, 0, q, a, wt, 8), arc(x + r, y + h - r, r, q, 2 * q, a, wt, 8), arc(x + r, y + r, r, 2 * q, 3 * q, a, wt, 8)];
  return line([...parts.flatMap((p) => p.pts), x + w - r, y], a, wt);
}
function quad(p0, c, p1, n = 24) {
  const pts = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n;
    const u = 1 - t;
    pts.push(u * u * p0[0] + 2 * u * t * c[0] + t * t * p1[0], u * u * p0[1] + 2 * u * t * c[1] + t * t * p1[1]);
  }
  return pts;
}

// The Altim logo (web/public/logo.svg, 1024 box centred on 512 and divided by 330): the peak and the rising line.
const logo = () => [
  line([-0.848, 0.812, 0, -0.903, 0.848, 0.812], 0, 1.15),
  line([-0.552, 0.267, -0.248, 0.024, 0.024, 0.191, 0.57, -0.279], 1, 1.25),
  disc(0.57, -0.279, 0.085, 1, 0.9),
];

// Eight daily candles, rising: a body and a wick each, the rising ones in the accent colour, on a base line.
function candles() {
  const oc = [[0.42, 0.26], [0.26, 0.36], [0.36, 0.12], [0.12, 0.2], [0.2, -0.06], [-0.06, -0.24], [-0.24, -0.12], [-0.12, -0.46]];
  const out = [line([-0.98, 0.72, 0.98, 0.72], 0, 0.5)];
  oc.forEach(([o, c], k) => {
    const x = -0.84 + k * 0.24;
    const up = c < o;
    const top = Math.min(o, c);
    const bottom = Math.max(o, c);
    out.push(rect(x - 0.065, top, 0.13, Math.max(bottom - top, 0.03), up ? 1 : 0, 1.1));
    out.push(line([x, top - 0.1 - (k % 3) * 0.03, x, top], up ? 1 : 0, 0.8));
    out.push(line([x, bottom, x, bottom + 0.08 + (k % 2) * 0.04], up ? 1 : 0, 0.8));
  });
  return out;
}

// A radar: three rings, a cross, the sweep and its trail in the accent colour, a few echoes.
function radar() {
  const s = -0.85;
  return [
    circle(0, 0, 0.86, 0, 1.2),
    circle(0, 0, 0.57, 0, 0.7),
    circle(0, 0, 0.28, 0, 0.5),
    line([-0.9, 0, 0.9, 0], 0, 0.35),
    line([0, -0.9, 0, 0.9], 0, 0.35),
    line([0, 0, 0.86 * Math.cos(s), 0.86 * Math.sin(s)], 1, 1.3),
    arc(0, 0, 0.86, s - 0.9, s, 1, 1.1, 24),
    arc(0, 0, 0.7, s - 0.6, s, 1, 0.5, 18),
    disc(0.36, -0.44, 0.045, 1, 1),
    disc(-0.48, 0.22, 0.035, 0, 0.7),
    disc(0.22, 0.52, 0.03, 0, 0.6),
  ];
}

// A balance: post, base, a slightly tilted beam, two pans on their chains; the pivot and one pan in the accent colour.
function verdict() {
  const pan = (x, y, a) => [
    line([x, y, x - 0.24, y + 0.46], a, 0.5),
    line([x, y, x + 0.24, y + 0.46], a, 0.5),
    arc(x, y + 0.46, 0.25, 0, Math.PI, a, 1.2, 28),
    line([x - 0.25, y + 0.46, x + 0.25, y + 0.46], a, 0.8),
  ];
  return [
    line([0, -0.66, 0, 0.74], 0, 1),
    line([-0.4, 0.74, 0.4, 0.74], 0, 1),
    line([-0.22, 0.74, -0.08, 0.62, 0.08, 0.62, 0.22, 0.74], 0, 0.6),
    line([-0.74, -0.48, 0.74, -0.6], 0, 1.1),
    disc(0, -0.54, 0.05, 1, 1),
    disc(0, -0.72, 0.03, 0, 0.6),
    ...pan(-0.74, -0.48, 0),
    ...pan(0.74, -0.6, 1),
  ];
}

// A shield with a check mark: nothing leaves your device, no order is placed.
function shield() {
  const outer = [0, -0.9, ...quad([0, -0.9], [-0.36, -0.7], [-0.7, -0.7]).slice(2), ...quad([-0.7, -0.7], [-0.72, 0.5], [0, 0.92]).slice(2), ...quad([0, 0.92], [0.72, 0.5], [0.7, -0.7]).slice(2), ...quad([0.7, -0.7], [0.36, -0.7], [0, -0.9]).slice(2)];
  const inner = [0, -0.76, ...quad([0, -0.76], [-0.3, -0.6], [-0.56, -0.6]).slice(2), ...quad([-0.56, -0.6], [-0.58, 0.4], [0, 0.76]).slice(2), ...quad([0, 0.76], [0.58, 0.4], [0.56, -0.6]).slice(2), ...quad([0.56, -0.6], [0.3, -0.6], [0, -0.76]).slice(2)];
  return [line(outer, 0, 1.2), line(inner, 0, 0.45), line([-0.27, -0.02, -0.06, 0.2, 0.3, -0.26], 1, 1.6)];
}

// The places Altim runs: a browser window, an iPhone and a watch, each with a small rising line.
function devices() {
  return [
    rrect(-1.0, -0.52, 0.98, 0.8, 0.05, 0, 1),
    line([-1.0, -0.4, -0.02, -0.4], 0, 0.6),
    disc(-0.94, -0.46, 0.014, 0, 0.3),
    disc(-0.9, -0.46, 0.014, 0, 0.3),
    disc(-0.86, -0.46, 0.014, 1, 0.3),
    line([-0.9, 0.16, -0.7, 0.02, -0.56, 0.08, -0.36, -0.14, -0.14, -0.22], 1, 1),
    rrect(0.1, -0.76, 0.46, 1.5, 0.08, 0, 1.1),
    line([0.27, -0.68, 0.39, -0.68], 0, 0.6),
    line([0.16, 0.1, 0.24, 0.0, 0.32, 0.05, 0.42, -0.14, 0.5, -0.2], 1, 0.9),
    line([0.17, 0.3, 0.49, 0.3], 0, 0.35),
    line([0.17, 0.42, 0.42, 0.42], 0, 0.35),
    line([0.17, 0.54, 0.46, 0.54], 0, 0.35),
    rrect(0.68, -0.12, 0.3, 0.36, 0.07, 0, 0.9),
    rect(0.73, -0.36, 0.2, 0.24, 0, 0.4),
    rect(0.73, 0.24, 0.2, 0.24, 0, 0.4),
    disc(0.83, 0.06, 0.04, 1, 0.7),
  ];
}

export const SHAPES = { logo, candles, radar, verdict, shield, devices };

// ---------- Sampling ----------

/** A small seeded random generator (mulberry32): the same grains on every visit. */
export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/**
 * `n` grains on the strokes of a shape: [x, y, accent] × n, spread evenly along the strokes (weighted), each grain a
 * little off its line (the grainy look), a few as loose dust; sorted by angle so that a morph sweeps instead of
 * crossing.
 */
export function sample(strokes, n, seed = 1) {
  const r = rng(seed);
  const segs = [];
  let total = 0;
  for (const s of strokes) {
    if (s.disc) {
      const len = Math.PI * 2 * s.disc[2] * 2.2 * s.w;
      segs.push({ disc: s.disc, a: s.a, len, at: total });
      total += len;
      continue;
    }
    for (let i = 0; i + 3 < s.pts.length; i += 2) {
      const [x0, y0, x1, y1] = [s.pts[i], s.pts[i + 1], s.pts[i + 2], s.pts[i + 3]];
      const len = Math.hypot(x1 - x0, y1 - y0) * s.w;
      if (len <= 0) continue;
      segs.push({ x0, y0, x1, y1, a: s.a, len, at: total });
      total += len;
    }
  }
  const out = new Float32Array(n * 3);
  const keys = new Float32Array(n);
  let k = 0;
  for (let i = 0; i < n; i++) {
    const u = ((i + r()) / n) * total;
    while (k + 1 < segs.length && segs[k + 1].at <= u) k++;
    const g = segs[k];
    let x;
    let y;
    if (g.disc) {
      const t = r() * Math.PI * 2;
      const d = Math.sqrt(r()) * g.disc[2];
      x = g.disc[0] + Math.cos(t) * d;
      y = g.disc[1] + Math.sin(t) * d;
    } else {
      const t = (u - g.at) / g.len;
      const nx = -(g.y1 - g.y0);
      const ny = g.x1 - g.x0;
      const nl = Math.hypot(nx, ny) || 1;
      const off = (r() + r() + r() - 1.5) * 0.016 * (r() < 0.05 ? 5 : 1);
      x = g.x0 + (g.x1 - g.x0) * t + (nx / nl) * off;
      y = g.y0 + (g.y1 - g.y0) * t + (ny / nl) * off;
    }
    out[i * 3] = x;
    out[i * 3 + 1] = y;
    out[i * 3 + 2] = g.a;
    keys[i] = Math.atan2(y, x) + r() * 0.05;
  }
  const order = Array.from(keys.keys()).sort((a, b) => keys[a] - keys[b]);
  const sorted = new Float32Array(n * 3);
  order.forEach((j, i) => sorted.set(out.subarray(j * 3, j * 3 + 3), i * 3));
  return sorted;
}

/** Same as altim_core::web::film::smoothstep. */
export const smoothstep = (a, b, x) => {
  const t = Math.min(Math.max((x - a) / (b - a), 0), 1);
  return t * t * (3 - 2 * t);
};

/** Same as altim_core::web::film::morph: the two shapes on screen at position `s` and the eased way between them. */
export function morph(s, n) {
  if (n < 2) return [0, 0, 0];
  s = Math.min(Math.max(s, 0), n - 1);
  const i = Math.min(Math.floor(s), n - 2);
  return [i, i + 1, smoothstep(0.25, 0.75, s - i)];
}

/** Grains for this device: fewer on a small or modest screen, more on a desktop. */
export function grainCount(width, cores, coarse) {
  if (coarse || width < 720) return cores && cores <= 4 ? 2000 : 2800;
  return cores && cores <= 4 ? 5000 : 7000;
}

// ---------- WebGL ----------

const VS = `
attribute vec3 a_from;
attribute vec3 a_to;
attribute vec4 a_seed;
uniform float u_t, u_time, u_scale, u_size, u_mirror, u_base;
uniform vec2 u_res, u_center;
varying float v_acc;
varying float v_alpha;
void main() {
  float t = clamp((u_t - a_seed.x * 0.3) / 0.7, 0.0, 1.0);
  t = t * t * (3.0 - 2.0 * t);
  vec2 p = mix(a_from.xy, a_to.xy, t);
  float burst = sin(3.14159 * t);
  float ang = a_seed.y * 6.2832 + u_time * 0.25;
  p += vec2(cos(ang), sin(ang)) * burst * (0.04 + 0.22 * a_seed.z * a_seed.z);
  p += 0.006 * vec2(sin(u_time * 0.9 + a_seed.y * 40.0), cos(u_time * 0.7 + a_seed.x * 40.0));
  float drift = step(0.5, a_seed.w);
  p.x = mix(p.x, mod(p.x + 1.8 + u_time * (0.01 + 0.03 * a_seed.z), 3.6) - 1.8, drift);
  float alpha = (0.4 + 0.6 * a_seed.z) * (0.78 + 0.22 * sin(u_time * 1.7 + a_seed.x * 50.0));
  alpha *= mix(1.0, 0.3, drift);
  if (u_mirror > 0.5) {
    p.y = u_base + (u_base - p.y) * 0.5;
    alpha *= 0.2 * clamp(1.0 - (p.y - u_base) / 0.9, 0.0, 1.0) * (1.0 - drift);
  }
  vec2 px = u_center + p * u_scale;
  gl_Position = vec4(px.x / u_res.x * 2.0 - 1.0, 1.0 - px.y / u_res.y * 2.0, 0.0, 1.0);
  gl_PointSize = u_size * (0.6 + 0.9 * a_seed.x);
  v_acc = mix(a_from.z, a_to.z, t);
  v_alpha = alpha;
}`;
const FS = `
precision mediump float;
varying float v_acc;
varying float v_alpha;
void main() {
  vec2 c = gl_PointCoord - 0.5;
  float r = dot(c, c) * 4.0;
  if (r > 1.0) discard;
  float a = (1.0 - r) * v_alpha;
  vec3 col = mix(vec3(${WHITE.join(",")}), vec3(${CYAN.join(",")}), v_acc);
  gl_FragColor = vec4(col * a, a);
}`;

function glScene(canvas, shapes, n) {
  const gl = canvas.getContext("webgl", { alpha: true, antialias: false, premultipliedAlpha: true, powerPreference: "low-power" });
  if (!gl) return null;
  const shader = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    return gl.getShaderParameter(s, gl.COMPILE_STATUS) ? s : null;
  };
  const vs = shader(gl.VERTEX_SHADER, VS);
  const fs = shader(gl.FRAGMENT_SHADER, FS);
  if (!vs || !fs) return null;
  const prog = gl.createProgram();
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return null;
  gl.useProgram(prog);
  const buffers = shapes.map((data) => {
    const b = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, b);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    return b;
  });
  const r = rng(7);
  const seeds = new Float32Array(n * 4);
  const drifting = Math.round(n * 0.06);
  for (let i = 0; i < n; i++) seeds.set([r(), r(), r(), i >= n - drifting ? 1 : 0], i * 4);
  const seedBuf = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, seedBuf);
  gl.bufferData(gl.ARRAY_BUFFER, seeds, gl.STATIC_DRAW);
  const at = (name) => gl.getAttribLocation(prog, name);
  const un = (name) => gl.getUniformLocation(prog, name);
  const loc = { from: at("a_from"), to: at("a_to"), seed: at("a_seed") };
  const u = {};
  for (const k of ["u_t", "u_time", "u_scale", "u_size", "u_mirror", "u_base", "u_res", "u_center"]) u[k] = un(k);
  gl.bindBuffer(gl.ARRAY_BUFFER, seedBuf);
  gl.enableVertexAttribArray(loc.seed);
  gl.vertexAttribPointer(loc.seed, 4, gl.FLOAT, false, 0, 0);
  gl.enableVertexAttribArray(loc.from);
  gl.enableVertexAttribArray(loc.to);
  gl.disable(gl.DEPTH_TEST);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE);
  gl.clearColor(0, 0, 0, 0);
  return {
    draw(view, from, to, t, time) {
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[from]);
      gl.vertexAttribPointer(loc.from, 3, gl.FLOAT, false, 0, 0);
      gl.bindBuffer(gl.ARRAY_BUFFER, buffers[to]);
      gl.vertexAttribPointer(loc.to, 3, gl.FLOAT, false, 0, 0);
      gl.uniform1f(u.u_t, t);
      gl.uniform1f(u.u_time, time);
      gl.uniform1f(u.u_scale, view.scale);
      gl.uniform1f(u.u_size, view.size);
      gl.uniform1f(u.u_base, 0.92);
      gl.uniform2f(u.u_res, canvas.width, canvas.height);
      gl.uniform2f(u.u_center, view.cx, view.cy);
      gl.uniform1f(u.u_mirror, 0);
      gl.drawArrays(gl.POINTS, 0, n);
      if (view.mirror) {
        gl.uniform1f(u.u_mirror, 1);
        gl.drawArrays(gl.POINTS, 0, n);
      }
    },
  };
}

// Canvas 2D without WebGL: the same morph on the CPU, fewer grains, no reflection.
function flatScene(canvas, shapes, n) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  const r = rng(7);
  const seeds = Array.from({ length: n }, () => [r(), r(), r()]);
  return {
    draw(view, from, to, t, time) {
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      const A = shapes[from];
      const B = shapes[to];
      for (let i = 0; i < n; i++) {
        const [d, g, z] = seeds[i];
        const tt = smoothstep(0, 1, (t - d * 0.3) / 0.7);
        const burst = Math.sin(Math.PI * tt) * (0.04 + 0.22 * z * z);
        const ang = g * 6.2832 + time * 0.25;
        const x = A[i * 3] + (B[i * 3] - A[i * 3]) * tt + Math.cos(ang) * burst;
        const y = A[i * 3 + 1] + (B[i * 3 + 1] - A[i * 3 + 1]) * tt + Math.sin(ang) * burst;
        const acc = A[i * 3 + 2] + (B[i * 3 + 2] - A[i * 3 + 2]) * tt;
        ctx.fillStyle = acc > 0.5 ? `rgba(0,240,255,${0.4 + 0.5 * z})` : `rgba(237,245,255,${0.3 + 0.5 * z})`;
        const s = view.size * (0.6 + 0.9 * d);
        ctx.fillRect(view.cx + x * view.scale - s / 2, view.cy + y * view.scale - s / 2, s, s);
      }
    },
  };
}

// ---------- The film ----------

function film(section) {
  if (section.dataset.motion) return;
  section.dataset.motion = "1";
  const names = (section.dataset.film || "").split(",").filter((s) => SHAPES[s]);
  if (REDUCED || section.classList.contains("still")) return stills(section);
  const saveData = navigator.connection && navigator.connection.saveData;
  const stage = section.querySelector(".film-stage");
  const canvas = section.querySelector(".film-canvas");
  if (saveData || !stage || !canvas || names.length < 2) return;
  const coarse = matchMedia("(pointer: coarse)").matches;
  let n = grainCount(innerWidth, navigator.hardwareConcurrency, coarse);
  const make = (count) => {
    const drifting = Math.round(count * 0.06);
    const r = rng(11);
    const dust = new Float32Array(drifting * 3);
    for (let i = 0; i < drifting; i++) dust.set([r() * 3.6 - 1.8, r() * 2.4 - 1.2, r() < 0.3 ? 1 : 0], i * 3);
    return names.map((name, k) => {
      const all = new Float32Array(count * 3);
      all.set(sample(SHAPES[name](), count - drifting, 101 + k));
      all.set(dust, (count - drifting) * 3);
      return all;
    });
  };
  let scene = glScene(canvas, make(n), n);
  if (!scene) {
    n = Math.min(n, 1200);
    scene = flatScene(canvas, make(n), n);
  }
  if (!scene) return;
  section.classList.add("film-live");
  const dpr = Math.min(devicePixelRatio || 1, 2);
  const view = { cx: 0, cy: 0, scale: 1, size: 2, mirror: true };
  const resize = () => {
    const w = stage.clientWidth;
    const h = stage.clientHeight;
    // Unchanged size (a phone's address bar sliding): the canvas is kept, not cleared.
    if (canvas.width === Math.round(w * dpr) && canvas.height === Math.round(h * dpr)) return;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    // The drawing sits in the upper part of the stage, the caption under it.
    const narrow = w < 720;
    view.scale = Math.min(w * (narrow ? 0.42 : 0.4), h * 0.26) * dpr;
    view.cx = (w / 2) * dpr;
    view.cy = h * (narrow ? 0.43 : 0.39) * dpr;
    view.size = (narrow ? 2.1 : 1.8) * dpr;
    view.mirror = h > 520;
  };
  resize();
  addEventListener("resize", resize, { passive: true });
  let shown = Number(stage.dataset.s) || 0;
  let last = 0;
  let raf = 0;
  let onScreen = false;
  const frame = (now) => {
    const started = performance.now();
    raf = 0;
    const dt = last ? Math.min((now - last) / 1000, 0.1) : 0.016;
    last = now;
    // The particles follow the scroll with a little inertia (the only smoothing: the page's scroll stays native).
    const target = Number(stage.dataset.s) || 0;
    shown += (target - shown) * (1 - Math.exp(-dt * 6));
    if (Math.abs(target - shown) < 0.0005) shown = target;
    const [a, b, t] = morph(shown, names.length);
    scene.draw(view, a, b, t, now / 1000);
    stats.frames++;
    stats.work += performance.now() - started;
    if (onScreen && !document.hidden) raf = requestAnimationFrame(frame);
  };
  const play = () => {
    if (!raf && onScreen && !document.hidden) {
      last = 0;
      raf = requestAnimationFrame(frame);
    }
  };
  new IntersectionObserver((e) => {
    onScreen = e[e.length - 1].isIntersecting;
    play();
  }).observe(section);
  document.addEventListener("visibilitychange", play);
}

/** Frames drawn and the time spent drawing them (ms), read by the performance check. */
export const stats = { frames: 0, work: 0 };

// Reduced motion: one still line drawing beside each caption.
function stills(section) {
  const dpr = Math.min(devicePixelRatio || 1, 2);
  section.querySelectorAll(".film-cap").forEach((li, k) => {
    const c = li.querySelector(".film-still");
    const make = SHAPES[li.dataset.shape];
    if (!c || !make) return;
    const w = c.clientWidth || 160;
    const h = c.clientHeight || 120;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const ctx = c.getContext("2d");
    if (!ctx) return;
    const s = Math.min(w * 0.42, h * 0.42) * dpr;
    const cx = (w / 2) * dpr;
    const cy = (h / 2) * dpr;
    const grains = sample(make(), 900, 101 + k);
    for (let i = 0; i < 900; i++) {
      ctx.fillStyle = grains[i * 3 + 2] > 0.5 ? "rgba(0,240,255,0.85)" : "rgba(237,245,255,0.7)";
      ctx.fillRect(cx + grains[i * 3] * s - dpr / 2, cy + grains[i * 3 + 1] * s - dpr / 2, 1.4 * dpr, 1.4 * dpr);
    }
    li.classList.add("drawn");
  });
}

// ---------- Fine pointers: soft cursor, magnetic buttons ----------

function pointer() {
  if (!FINE || REDUCED) return;
  const ring = document.createElement("div");
  ring.className = "cursor";
  ring.setAttribute("aria-hidden", "true");
  ring.style.transform = "translate3d(-100px,-100px,0)";
  document.body.appendChild(ring);
  let x = -100;
  let y = -100;
  let cx = x;
  let cy = y;
  let raf = 0;
  const step = () => {
    cx += (x - cx) * 0.22;
    cy += (y - cy) * 0.22;
    ring.style.transform = `translate3d(${cx}px,${cy}px,0)`;
    raf = Math.abs(x - cx) + Math.abs(y - cy) > 0.3 ? requestAnimationFrame(step) : 0;
  };
  addEventListener(
    "pointermove",
    (e) => {
      if (e.pointerType !== "mouse") return;
      x = e.clientX;
      y = e.clientY;
      ring.classList.add("on");
      const t = e.target instanceof Element ? e.target.closest("a, button, summary, [role=button]") : null;
      ring.classList.toggle("link", !!t);
      if (!raf) raf = requestAnimationFrame(step);
    },
    { passive: true },
  );
  document.addEventListener("pointerleave", () => ring.classList.remove("on"));
  let magnet = null;
  const release = (b) => {
    b.style.transform = "";
    magnet = null;
  };
  // Magnetic call-to-action buttons: they lean towards the pointer, at most 10 px.
  addEventListener(
    "pointermove",
    (e) => {
      const b = e.target instanceof Element ? e.target.closest(".btn") : null;
      if (magnet && magnet !== b) release(magnet);
      if (!b || e.pointerType !== "mouse") return;
      magnet = b;
      const r = b.getBoundingClientRect();
      const dx = (e.clientX - (r.left + r.width / 2)) / (r.width / 2);
      const dy = (e.clientY - (r.top + r.height / 2)) / (r.height / 2);
      b.classList.add("magnet");
      b.style.transform = `translate3d(${(dx * 8).toFixed(1)}px,${(dy * 5).toFixed(1)}px,0)`;
    },
    { passive: true },
  );
}

// ---------- Start ----------

function scan() {
  document.querySelectorAll("[data-film]").forEach(film);
}
// In a page (not when frontend/motion.test.mjs imports the pure functions under Node).
if (BROWSER) {
  scan();
  pointer();
  // The site's pages are drawn by the .wasm: a film shown later (another page of the site) starts when it appears.
  const root = document.getElementById("root");
  if (root) new MutationObserver(scan).observe(root, { childList: true, subtree: true });
}
