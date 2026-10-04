import assert from "node:assert/strict";
import test from "node:test";
import {
  compareEffectiveFrames,
  dominantImageRgba,
  rasterFixtureSource,
  resolveQualifiedBackend,
} from "./manim-raster-support.mjs";

const frame = { objects: [{ id: 7, present: true, transform: { y: 3.9542133808135986 }, opacity: 0.5 }] };

test("automatic paired qualification records either actual supported backend", () => {
  assert.equal(resolveQualifiedBackend("automatic", "WebGPU"), "WebGPU");
  assert.equal(resolveQualifiedBackend("automatic", "WebGL2"), "WebGL2");
});

test("explicit paired qualification retains strict backend matching", () => {
  assert.equal(resolveQualifiedBackend("WebGL2", "WebGL2"), "WebGL2");
  assert.throws(() => resolveQualifiedBackend("WebGL2", "WebGPU"), /expected WebGL2/);
  assert.throws(() => resolveQualifiedBackend("automatic", "Other"), /unsupported backend/);
  assert.throws(() => resolveQualifiedBackend("Other", "WebGPU"), /unsupported expected/);
});

test("full-image background estimate ignores a colored perimeter", () => {
  const width = 7;
  const height = 7;
  const background = [12, 34, 56, 255];
  const perimeter = [180, 40, 220, 255];
  const data = Buffer.alloc(width * height * 4);
  for (let offset = 0; offset < data.length; offset += 4) {
    data.set(background, offset);
  }
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      if (x === 0 || y === 0 || x === width - 1 || y === height - 1) {
        data.set(perimeter, (y * width + x) * 4);
      }
    }
  }
  assert.deepEqual(dominantImageRgba({ data, width, height }), background);
});

test("raster fixture adaptation leaves default sources without LaTeX preparation", () => {
  const source = "from manim import *\nclass Example(Scene):\n    pass\n";
  const adapted = rasterFixtureSource(source, "Example");
  assert.equal(adapted.match(/prepare_latex/g), null);
  assert.match(adapted, /^from noon import \*/);
});

test("raster fixture adaptation prepares LaTeX exactly once when requested", () => {
  const source = "from manim import *\nclass Example(Scene):\n    pass\n";
  const adapted = rasterFixtureSource(source, "Example", { requires_latex: true });
  assert.equal(adapted.match(/from noon import prepare_latex/g)?.length, 1);
  assert.equal(adapted.match(/await prepare_latex\(\)/g)?.length, 1);
  assert.match(adapted, /^from noon import prepare_latex\nawait prepare_latex\(\)\nfrom noon import \*/);
});

test("effective comparison accepts f32 rounding and reports the actual error", () => {
  const actual = structuredClone(frame);
  actual.objects[0].transform.y = 3.9542136192321777;
  assert.equal(compareEffectiveFrames(actual, frame), 2.384185791015625e-7);
  assert.equal(compareEffectiveFrames(frame, structuredClone(frame)), 0);
});

test("effective comparison rejects changed properties and non-finite values", () => {
  for (const value of [0.501, NaN, Infinity]) {
    const actual = structuredClone(frame);
    actual.objects[0].opacity = value;
    assert.throws(() => compareEffectiveFrames(actual, frame), /effective value differs/);
  }
});

test("effective comparison preserves identity, presence and object structure", () => {
  for (const mutate of [
    (f) => { f.objects[0].id += 1; },
    (f) => { f.objects[0].present = false; },
    (f) => { delete f.objects[0].opacity; },
    (f) => { f.objects.push(structuredClone(f.objects[0])); },
    (f) => { f.objects = { 0: f.objects[0] }; },
  ]) {
    const actual = structuredClone(frame);
    mutate(actual);
    assert.throws(() => compareEffectiveFrames(actual, frame));
  }
});
