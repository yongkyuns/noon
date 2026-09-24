import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { CASES, TIMES, assertWitnesses, validateReport } from "./foreground-matching-checks.mjs";

const manifest = JSON.parse(await readFile(new URL(
  "../parity/manim-v0.21/foreground-matching-manifest.json", import.meta.url), "utf8"));
function report() {
  return { reference: structuredClone(manifest.reference), fixtures: manifest.fixtures.map(f => ({
    id: f.id, scene: f.scene, expectedDuration: 2.4,
    backends: Object.fromEntries(["webgpu", "webgl"].map(backend => [backend, {
      noonDuration: 2.4, durationDelta: 0,
      samples: TIMES.map((time, i) => ({ time, frameIndex: i,
        debugFrame: { engine: "noon", time } })),
    }])),
  })) };
}

// Synthetic images only test the checker. Real fixture rendering happens in CI.
function patch(png, x, y, rgb, alpha = 255) {
  const cx = Math.round(480 + x * 67.5), cy = Math.round(270 - y * 67.5);
  for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) {
    png.data.set([...rgb, alpha], ((cy + dy) * 960 + cx + dx) * 4);
  }
}
function witnessImage(id, time) {
  const png = { width: 960, height: 540, data: Buffer.alloc(960 * 540 * 4) };
  for (let i = 3; i < png.data.length; i += 4) png.data[i] = 255;
  const mixed = [0, Math.round(255 * time / 2), Math.round(255 * (1 - time / 2))];
  patch(png, -2.4, 0, id === "matching-foreground-source" && time < 2 ? mixed : [255, 255, 255]);
  patch(png, 2.4, 0, [255, 255, 255]);
  if (time < 2) patch(png, -2.4, -0.4, mixed);
  else if (time < 2.2) patch(png, 0.8, -0.4, [0, 255, 0]);
  if (time >= 0.5 && time < 2.2) patch(png, 2.4, -0.4, [0, 128, 0]);
  if (time >= 2.2) patch(png, 0, -0.35, [255, 0, 0]);
  return png;
}

test("all three cases require complete, aligned evidence on both backends", () => {
  validateReport(manifest, report());
  for (const [id] of CASES) for (const time of TIMES) assertWitnesses(witnessImage(id, time), id, time);
});
test("a missing backend is not a passing empty comparison", () => {
  const bad = report(); delete bad.fixtures[0].backends.webgpu;
  assert.throws(() => validateReport(manifest, bad), /missing backend/);
});
test("an incomplete time series is rejected", () => {
  const bad = report(); bad.fixtures[1].backends.webgl.samples.pop();
  assert.throws(() => validateReport(manifest, bad), /missing\/extra samples/);
});
test("duplicate frames cannot substitute for distinct samples", () => {
  const bad = report(); bad.fixtures[1].backends.webgpu.samples[1].frameIndex = 0;
  assert.throws(() => validateReport(manifest, bad), /duplicate frames/);
});
test("stale frame receipts fail even when image witnesses could pass", () => {
  const bad = report(); bad.fixtures[1].backends.webgpu.samples[1].debugFrame.time = 0;
  assert.throws(() => validateReport(manifest, bad), /stale effective frame/);
});
test("an ordinary source above foreground is detected", () => {
  const id = CASES[0][0], png = witnessImage(id, 1);
  patch(png, -2.4, 0, [0, 128, 128]);
  assert.throws(() => assertWitnesses(png, id, 1), /matched-source\/foreground order/);
});
test("a foreground source moved behind its earlier foreground sibling is detected", () => {
  const id = CASES[1][0], png = witnessImage(id, 1);
  patch(png, -2.4, 0, [255, 255, 255]);
  assert.throws(() => assertWitnesses(png, id, 1), /matched-source\/foreground order/);
});
test("a target-only occurrence above the foreground-only anchor is detected", () => {
  const id = CASES[2][0], png = witnessImage(id, 1);
  patch(png, 2.4, 0, [0, 255, 0]);
  assert.throws(() => assertWitnesses(png, id, 1), /target-only occurrence covered/);
});
test("hiding a target-only shape cannot fake correct occlusion", () => {
  const id = CASES[2][0], png = witnessImage(id, 1);
  patch(png, 2.4, -0.4, [0, 0, 0]);
  assert.throws(() => assertWitnesses(png, id, 1), /target-only shape actually exists/);
});
test("a frozen source cannot pass intermediate interpolation", () => {
  const id = CASES[0][0], png = witnessImage(id, 1);
  patch(png, -2.4, -0.4, [0, 0, 255]);
  assert.throws(() => assertWitnesses(png, id, 1), /real interpolation/);
});
test("completion must retain the extra target shape", () => {
  const id = CASES[0][0], png = witnessImage(id, 2);
  patch(png, 0.8, -0.4, [0, 0, 0]);
  assert.throws(() => assertWitnesses(png, id, 2), /padded target survives/);
});
test("later add must actually be visible", () => {
  const id = CASES[0][0], png = witnessImage(id, 2.2);
  patch(png, 0, -0.35, [0, 0, 0]);
  assert.throws(() => assertWitnesses(png, id, 2.2), /later ordinary addition/);
});
test("transparent witness pixels do not pass a colour-only check", () => {
  const id = CASES[0][0], png = witnessImage(id, 1);
  patch(png, 2.4, 0, [255, 255, 255], 0);
  assert.throws(() => assertWitnesses(png, id, 1), /target-only occurrence covered/);
});
test("reference version, incomplete fixture reports and worker errors stay blocking", () => {
  const wrongVersion = report(); wrongVersion.reference.version = "0.20.0";
  assert.throws(() => validateReport(manifest, wrongVersion), /reference provenance changed/);
  const missing = report(); missing.fixtures.pop();
  assert.throws(() => validateReport(manifest, missing), /incomplete report/);
  const crashed = report(); crashed.fixtures[0].backends.webgpu.error = "worker crashed";
  assert.throws(() => validateReport(manifest, crashed), /worker crashed/);
});
