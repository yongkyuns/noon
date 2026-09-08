import assert from "node:assert/strict";
import { test } from "node:test";
import { verifySample, verifySquareToCircle, verifyFreshRun, verifyLateFamilyConstruction } from "./semantic-preview-observations.mjs";

function frames() {
  return new Map([0, 30, 45, 60, 90].map((index, i) => [index, {
    sample: { error: null, presented: true, frameIndex: index, time: index / 30,
      rendererBackend: "WebGL2", objectCount: index === 90 ? 0 : 1, authoredDuration: index === 90 ? 3 : null },
    imageSha256: String(i === 4 ? 0 : i).repeat(64),
    foreground: { count: index === 0 || index === 90 ? 0 : 1000,
      width: index === 30 ? 190 : 136, centerDistance: index === 30 ? 0 : 80 },
  }]));
}

test("valid capture schedule and same-backend fresh run pass", () => {
  const first = frames();
  for (const [index, frame] of first) verifySample(frame.sample, index, "WebGL2");
  verifySquareToCircle(first);
  verifyFreshRun(first, frames());
});
test("wrong time, backend, index, error, presentation and malformed membership fail", () => {
  for (const replacement of [{time: 99}, {rendererBackend: "WebGPU"}, {frameIndex: 60},
    {error: "lost"}, {presented: false}, {objectCount: null}, {objectCount: -1}]) {
    assert.throws(() => verifySample({...frames().get(30).sample, ...replacement}, 30, "WebGL2"));
  }
});
test("missing intermediate evidence fails", () => {
  const value = frames(); value.delete(45);
  assert.throws(() => verifySquareToCircle(value), /missing preview frame/);
});
test("transition frozen at either endpoint fails", () => {
  for (const endpoint of [30, 60]) {
    const value = frames(); value.get(45).imageSha256 = value.get(endpoint).imageSha256;
    assert.throws(() => verifySquareToCircle(value), /morph cannot/);
  }
});
test("blank intermediate image fails despite valid endpoints", () => {
  const value = frames(); value.get(45).foreground.count = 0;
  assert.throws(() => verifySquareToCircle(value), /visible through the transition/);
});
test("wrong initial target shape fails", () => {
  const value = frames(); value.get(30).foreground.centerDistance = 80;
  assert.throws(() => verifySquareToCircle(value), /hollow/);
});
test("empty final image cannot hide incomplete lifecycle or duration", () => {
  for (const replacement of [{objectCount: 1}, {authoredDuration: null}, {authoredDuration: 2}]) {
    const value = frames(); Object.assign(value.get(90).sample, replacement);
    assert.throws(() => verifySquareToCircle(value));
  }
});
test("fresh-run comparison rejects leaked visual state and a changed schedule", () => {
  const before = frames(), after = frames();
  after.get(45).imageSha256 = "a".repeat(64);
  assert.throws(() => verifyFreshRun(before, after), /changed frame/);
  after.delete(45);
  assert.throws(() => verifyFreshRun(before, after), /same frame schedule/);
});

function lateFamilyFrames() {
  return new Map([0, 30, 63, 96].map((index, i) => [index, {
    sample: { objectCount: index === 0 ? 2 : 4, authoredDuration: index === 96 ? 3.2 : null },
    imageSha256: String(i).repeat(64),
    foreground: { count: index === 0 || index === 30 ? 0 : 1000 },
  }]));
}

test("late family capture spans the original Succession and subsequent fade", () => {
  verifyLateFamilyConstruction(lateFamilyFrames());
});
test("late family evidence cannot omit a sampled phase or blank visible fade state", () => {
  for (const index of [0, 30, 63, 96]) {
    const value = lateFamilyFrames(); value.delete(index);
    assert.throws(() => verifyLateFamilyConstruction(value), /missing/);
  }
  for (const index of [63, 96]) {
    const value = lateFamilyFrames(); value.get(index).foreground.count = 0;
    assert.throws(() => verifyLateFamilyConstruction(value), /visible/);
  }
});
test("late family evidence rejects premature visibility, wrong boundary admission and frozen composition", () => {
  const premature = lateFamilyFrames(); premature.get(0).foreground.count = 100;
  assert.throws(() => verifyLateFamilyConstruction(premature), /initial Wait/);
  const visibleBoundary = lateFamilyFrames(); visibleBoundary.get(30).foreground.count = 100;
  assert.throws(() => verifyLateFamilyConstruction(visibleBoundary), /transparent/);
  const wrongMembership = lateFamilyFrames(); wrongMembership.get(30).sample.objectCount = 2;
  assert.throws(() => verifyLateFamilyConstruction(wrongMembership), /admit all four/);
  for (const endpoint of [30, 96]) {
    const value = lateFamilyFrames(); value.get(63).imageSha256 = value.get(endpoint).imageSha256;
    assert.throws(() => verifyLateFamilyConstruction(value), /cannot/);
  }
});
test("late family completion needs exact duration and all four objects", () => {
  for (const replacement of [{objectCount: 2}, {objectCount: null},
    {authoredDuration: null}, {authoredDuration: 1}, {authoredDuration: NaN}]) {
    const value = lateFamilyFrames(); Object.assign(value.get(96).sample, replacement);
    assert.throws(() => verifyLateFamilyConstruction(value));
  }
});
