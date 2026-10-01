import assert from "node:assert/strict";
import test from "node:test";
import { shouldSampleRendererStageFrame } from "./renderer-stage-sampling.js";

test("renderer stage sample frames span a long measurement in at most 32 slots", () => {
  const frameCount = 1_140;
  const sampledFrames = Array.from({ length: frameCount }, (_, frame) => frame)
    .filter((frame) => shouldSampleRendererStageFrame(frame, frameCount));
  assert.equal(sampledFrames.length, 32);
  assert.ok(sampledFrames[0] > 0);
  assert.equal(sampledFrames.at(-1), frameCount - 1);
  assert.ok(sampledFrames.every((frame, index) =>
    index === 0 || frame - sampledFrames[index - 1] >= 35));
  assert.ok(sampledFrames.at(-1) - sampledFrames[0] > frameCount * 0.95);
});

test("short renderer stage measurements select every measured frame", () => {
  assert.deepEqual(
    Array.from({ length: 4 }, (_, frame) => frame)
      .filter((frame) => shouldSampleRendererStageFrame(frame, 4)),
    [0, 1, 2, 3],
  );
});
