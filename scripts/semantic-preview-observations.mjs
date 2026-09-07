// Assertions for the trusted SquareToCircle preview fixture, not engine semantics.
import assert from "node:assert/strict";

export function verifySample(sample, frameIndex, expectedBackend) {
  assert.equal(sample.error, null, "preview sample must not hide a renderer error");
  assert.equal(sample.presented, true, "preview must acknowledge presentation");
  assert.equal(sample.frameIndex, frameIndex, "preview returned another frame");
  assert.equal(sample.time, frameIndex / 30, "preview returned another authored time");
  assert.equal(sample.rendererBackend, expectedBackend, "preview selected another backend");
  assert.ok(Number.isSafeInteger(sample.objectCount) && sample.objectCount >= 0,
    "preview sample requires a valid object count");
}

export function verifySquareToCircle(frames) {
  for (const index of [0, 30, 45, 60, 90]) {
    assert.ok(frames.has(index), `missing preview frame ${index}`);
    const frame = frames.get(index);
    assert.match(frame.imageSha256, /^[0-9a-f]{64}$/, "frame must identify its captured PNG");
  }
  const initial = frames.get(0), square = frames.get(30), middle = frames.get(45);
  const circle = frames.get(60), final = frames.get(90);
  assert.equal(initial.foreground.count, 0, "Create begins before the square is revealed");
  for (const frame of [square, middle, circle]) {
    assert.equal(frame.sample.objectCount, 1);
    assert.ok(frame.foreground.count > 20, "shape must be visible through the transition");
  }
  assert.ok(square.foreground.centerDistance < 12, "initial square must be hollow");
  assert.ok(circle.foreground.centerDistance > 24, "target circle must be filled");
  assert.ok(square.foreground.width > circle.foreground.width * 1.2,
    "rotated square must be wider than the endpoint circle");
  assert.notEqual(middle.imageSha256, square.imageSha256, "morph cannot freeze at the square");
  assert.notEqual(middle.imageSha256, circle.imageSha256, "morph cannot jump to the circle");
  assert.equal(final.sample.objectCount, 0, "FadeOut must detach its object");
  assert.equal(final.foreground.count, 0, "FadeOut must leave an empty final image");
  assert.equal(final.sample.authoredDuration, 3, "source must complete the full three-second scene");
}

export function verifyFreshRun(before, after) {
  assert.deepEqual([...after.keys()], [...before.keys()], "fresh run must capture the same frame schedule");
  for (const [index, frame] of before) {
    assert.equal(after.get(index).imageSha256, frame.imageSha256,
      `fresh run changed frame ${index} on the same backend`);
    assert.equal(after.get(index).sample.objectCount, frame.sample.objectCount,
      `fresh run changed membership at frame ${index}`);
  }
}

// The original fixture finishes its initial Succession at 1 s, then fades the
// two late-created squares over 2.2 s. This checks captured evidence, not a
// second scheduler or scene model.
export function verifyLateFamilyConstruction(frames) {
  for (const index of [0, 30, 63, 96]) {
    assert.ok(frames.has(index), `missing late-family preview frame ${index}`);
    assert.match(frames.get(index).imageSha256, /^[0-9a-f]{64}$/,
      "late-family frame must identify its captured PNG");
  }
  const initial = frames.get(0), beforeFade = frames.get(30);
  const middle = frames.get(63), final = frames.get(96);
  assert.equal(initial.foreground.count, 0, "initial Wait must not show future objects");
  for (const frame of [beforeFade, middle, final]) {
    assert.ok(frame.foreground.count > 20, "late-family scene must remain visible");
  }
  assert.notEqual(middle.imageSha256, beforeFade.imageSha256,
    "late-family fade cannot remain at its initial image");
  assert.notEqual(middle.imageSha256, final.imageSha256,
    "late-family fade cannot jump to its final image");
  assert.equal(final.sample.objectCount, 4,
    "both original circles and late-created squares must remain");
  assert.ok(Number.isFinite(final.sample.authoredDuration) &&
    Math.abs(final.sample.authoredDuration - 3.2) < 1e-9,
    "source must finish the second composition");
}
