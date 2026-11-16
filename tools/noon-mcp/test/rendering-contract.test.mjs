import assert from "node:assert/strict";
import { test } from "node:test";

import {
  MAX_RENDER_SAMPLES_PER_CALL,
  MAX_RENDER_SOURCE_BYTES,
  MAX_RENDER_TIME_SECONDS,
  RENDERING_CONTRACT_VERSION,
  RENDERING_TOOL_NAMES,
  renderingToolContracts,
} from "../src/rendering-contract.mjs";

const session = "scene_0123456789abcdef";

function accepts(name, value) {
  return renderingToolContracts[name].inputSchema.safeParse(value).success;
}

test("rendering contract is versioned and names the preview and feedback operations", () => {
  assert.equal(RENDERING_CONTRACT_VERSION, 2);
  assert.deepEqual(RENDERING_TOOL_NAMES, [
    "noon_open_scene", "noon_sample_frames", "noon_inspect", "noon_annotate_frame", "noon_close_scene",
  ]);
});

test("open scene accepts only bounded source and loop duration", () => {
  assert.equal(accepts("noon_open_scene", { source: "from noon import *\n" }), true);
  assert.equal(accepts("noon_open_scene", {
    source: "from noon import *\n", loopDurationSeconds: 4,
  }), true);
  for (const invalid of [
    { source: "" },
    { source: "   \n" },
    { source: "x\0y" },
    { source: "x".repeat(MAX_RENDER_SOURCE_BYTES + 1) },
    { source: "é".repeat(Math.floor(MAX_RENDER_SOURCE_BYTES / 2) + 1) },
    { source: "x", loopDurationSeconds: 0 },
    { source: "x", loopDurationSeconds: Infinity },
    { source: "x", loopDurationSeconds: MAX_RENDER_TIME_SECONDS + 1 },
    { source: "x", repoRoot: "/tmp/other" },
    { source: "x", command: "python" },
    { source: "x", cwd: "/" },
  ]) {
    assert.equal(accepts("noon_open_scene", invalid), false, JSON.stringify(invalid));
  }
});

test("sample frames requires an opaque session and monotonic bounded forward schedule", () => {
  assert.equal(MAX_RENDER_SAMPLES_PER_CALL, 31,
    "one call plus open_scene's initial frame must fit the default 32-frame retention budget");
  assert.equal(accepts("noon_sample_frames", { session, times: [0, 0.5, 0.5, 4] }), true);
  assert.equal(accepts("noon_sample_frames", {
    session, times: Array(MAX_RENDER_SAMPLES_PER_CALL).fill(0),
  }), true);
  for (const invalid of [
    { session: "short", times: [0] },
    { session: "scene with spaces 123456789", times: [0] },
    { session, times: [] },
    { session, times: [1, 0.5] },
    { session, times: [-1] },
    { session, times: [Infinity] },
    { session, times: [MAX_RENDER_TIME_SECONDS + 0.001] },
    { session, times: Array(MAX_RENDER_SAMPLES_PER_CALL + 1).fill(0) },
    { session, times: [0], seek: true },
    { session, times: [0], backend: "webgpu" },
  ]) {
    assert.equal(accepts("noon_sample_frames", invalid), false, JSON.stringify(invalid));
  }
});

test("inspect and close accept only the session capability", () => {
  for (const name of ["noon_inspect", "noon_close_scene"]) {
    assert.equal(accepts(name, { session }), true);
    assert.equal(accepts(name, { session, repoRoot: "/" }), false);
    assert.equal(accepts(name, { session, arbitrarySceneQuery: "all objects" }), false);
  }
  assert.equal(renderingToolContracts.noon_inspect.annotations.readOnlyHint, true);
  assert.equal(renderingToolContracts.noon_close_scene.annotations.destructiveHint, true);
  assert.equal(renderingToolContracts.noon_close_scene.annotations.idempotentHint, false,
    "a closed handle becomes stale and must be rejected rather than silently reused");
});

test("frame feedback accepts bounded reviewer input but no caller-supplied provenance or status", () => {
  const input = {
    session,
    artifactId: "artifact_0123456789abcdef",
    interval: { start: 1, end: 2 },
    observation: "The input arrows disappear when the sum reaches zero.",
    desiredChange: "Keep both inputs visible and update only the sum.",
  };
  assert.equal(accepts("noon_annotate_frame", input), true);
  assert.equal(accepts("noon_annotate_frame", {
    ...input,
    region: { x: 0.25, y: 0.1, width: 0.5, height: 0.8 },
    preserve: "The two contributions still exist.",
  }), true);
  for (const extra of [
    { session: "short" },
    { artifactId: "short" },
    { interval: { start: -1, end: 2 } },
    { interval: { start: 2, end: 1 } },
    { interval: { start: 0, end: Infinity } },
    { interval: { start: 0, end: MAX_RENDER_TIME_SECONDS + 1 } },
    { interval: { start: 1, end: 2, target: "sum" } },
    { region: { x: 0, y: 0, width: 0, height: 1 } },
    { region: { x: 0.75, y: 0, width: 0.5, height: 1 } },
    { region: { x: 0, y: 0.5, width: 1, height: 0.75 } },
    { observation: " " },
    { observation: "é".repeat(2001) },
    { desiredChange: "x".repeat(4001) },
    { preserve: "x".repeat(4001) },
    { status: "resolved" },
    { provenance: { sourceSha256: "f".repeat(64) } },
    { sourceSha256: "f".repeat(64) },
    { frame: { id: input.artifactId } },
    { semanticNodeId: "new-object" },
  ]) {
    assert.equal(accepts("noon_annotate_frame", { ...input, ...extra }), false,
      JSON.stringify(extra));
  }
  assert.deepEqual(renderingToolContracts.noon_annotate_frame.annotations, {
    readOnlyHint: true,
    destructiveHint: false,
    idempotentHint: true,
    openWorldHint: false,
  });
});
