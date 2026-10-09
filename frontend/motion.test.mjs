// Pure functions of frontend/motion.js (node --test frontend/motion.test.mjs): the morph matches
// altim_core::web::film::morph (same cases as its Rust tests), every shape is drawn by grains near its strokes.
import assert from "node:assert/strict";
import test from "node:test";
import { SHAPES, grainCount, morph, rng, sample } from "./motion.js";

test("morph: same as altim_core::web::film::morph", () => {
  assert.deepEqual(morph(0, 7), [0, 1, 0]);
  assert.deepEqual(morph(0.2, 7), [0, 1, 0]);
  assert.deepEqual(morph(0.5, 7), [0, 1, 0.5]);
  assert.deepEqual(morph(0.8, 7), [0, 1, 1]);
  assert.deepEqual(morph(3, 7), [3, 4, 0]);
  assert.deepEqual(morph(6, 7), [5, 6, 1]);
  assert.deepEqual(morph(7.5, 7), [5, 6, 1]);
  assert.deepEqual(morph(-1, 7), [0, 1, 0]);
  assert.deepEqual(morph(3, 1), [0, 0, 0]);
});

test("the stations' shapes exist (altim_core::web::film::STATIONS)", () => {
  for (const name of ["logo", "candles", "radar", "verdict", "shield", "devices"]) assert.equal(typeof SHAPES[name], "function", name);
});

test("grains sit on the drawing, some in the accent colour, the same on every visit", () => {
  for (const [name, make] of Object.entries(SHAPES)) {
    const g = sample(make(), 3000, 5);
    assert.equal(g.length, 9000);
    let accent = 0;
    for (let i = 0; i < 3000; i++) {
      const [x, y, a] = [g[i * 3], g[i * 3 + 1], g[i * 3 + 2]];
      assert.ok(Math.abs(x) <= 1.15 && Math.abs(y) <= 1.15, `${name}: (${x}, ${y}) hors du cadre`);
      accent += a;
    }
    assert.ok(accent > 50 && accent < 2700, `${name}: ${accent} grains en couleur`);
    assert.deepEqual(sample(make(), 3000, 5), g);
  }
  const r = rng(1);
  assert.notEqual(r(), r());
});

test("fewer grains on a phone than on a desktop", () => {
  assert.ok(grainCount(390, 8, true) <= 3000);
  assert.ok(grainCount(390, 4, true) < grainCount(390, 8, true));
  assert.ok(grainCount(1440, 8, false) >= 6000 && grainCount(1440, 8, false) <= 8000);
  assert.ok(grainCount(1440, 4, false) < grainCount(1440, 8, false));
});
