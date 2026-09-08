import assert from "node:assert/strict";
import test from "node:test";
import { compareEffectiveFrames } from "./manim-raster-support.mjs";

const frame = { objects: [{ id: 7, present: true, transform: { y: 3.9542133808135986 }, opacity: 0.5 }] };

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
