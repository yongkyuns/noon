import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const source = readFileSync(
  new URL("./authoring-render-worker.js", import.meta.url),
  "utf8",
);

function functionSlice(name, nextName) {
  const start = source.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `missing ${name}`);
  const end = source.indexOf(`function ${nextName}`, start + 1);
  assert.notEqual(end, -1, `missing ${nextName}`);
  return source.slice(start, end);
}

test("renderer transition keeps the active renderer until replacement bootstrap arrives", () => {
  const begin = functionSlice("beginRendererTransition", "resize");
  assert.match(begin, /transitionMode = nextMode;/);
  assert.doesNotMatch(begin, /disposeRenderer\(\)/);
  assert.doesNotMatch(begin, /mode = nextMode;/);
  assert.doesNotMatch(begin, /needsPresent = false;/);

  const consume = functionSlice("consumeDelta", "commitRendererTransition");
  assert.match(
    consume,
    /if \(transitionMode !== null\) \{\s*return commitRendererTransition\(json, publication\);/s,
  );

  const commit = functionSlice("commitRendererTransition", "bootstrapRenderer");
  const disposeIndex = commit.indexOf("disposeRenderer();");
  const publishModeIndex = commit.indexOf("mode = nextMode;");
  const bootstrapIndex = commit.indexOf(
    "bootstrapPromise = bootstrapRenderer(initial, resumeFrameLoop, publication);",
  );
  assert.ok(disposeIndex >= 0, "transition commit must retire the previous renderer");
  assert.ok(
    publishModeIndex > disposeIndex,
    "next mode must not publish before the previous renderer is retired",
  );
  assert.ok(
    bootstrapIndex > publishModeIndex,
    "replacement renderer bootstrap must start only after commit state is published",
  );
});

test("retained transition resources stage separately from the active renderer", () => {
  const resources = functionSlice("handleRetainedResources", "drainTransport");
  assert.match(resources, /if \(transitionMode !== null\)/);
  assert.match(resources, /transitionMode !== MODE_RETAINED/);
  assert.match(resources, /transitionResourceBytes = message\.bytes;/);

  const commit = functionSlice("commitRendererTransition", "bootstrapRenderer");
  assert.match(
    commit,
    /nextMode === MODE_RETAINED && transitionResourceBytes === null/,
  );
  assert.match(commit, /const nextResourceBytes = transitionResourceBytes;/);
  assert.match(commit, /resourceBytes = nextResourceBytes;/);
});

test("renderer transitions suspend ticks until delayed bootstrap publishes ready", () => {
  const begin = functionSlice("beginRendererTransition", "resize");
  const capture = begin.indexOf("transitionFrameLoopWasRunning = running;");
  const suspend = begin.indexOf("running = false;");
  const attach = begin.indexOf("attachRenderPort(message.port);");
  assert.ok(capture >= 0 && suspend > capture && attach > suspend);

  const commit = functionSlice("commitRendererTransition", "bootstrapRenderer");
  assert.match(commit, /const resumeFrameLoop = transitionFrameLoopWasRunning;/);
  assert.match(commit, /bootstrapRenderer\(initial, resumeFrameLoop, publication\)/);

  const bootstrap = functionSlice("bootstrapRenderer", "tryPresent");
  const create = bootstrap.indexOf("await RetainedExecutionCanvasRenderer.create");
  const retryPresent = bootstrap.indexOf("while (!tryPresent())");
  const ready = bootstrap.indexOf("const ready = {");
  const schedule = bootstrap.indexOf("scheduleFrame(bootstrapGeneration);");
  assert.ok(create >= 0 && retryPresent > create && ready > retryPresent && schedule > ready);
  assert.match(
    bootstrap,
    /await nextRenderOpportunity\(\);[\s\S]*?bootstrapGeneration !== frameLoopGeneration/,
  );
  assert.match(bootstrap, /if \(resumeFrameLoop\) \{\s*running = true;\s*scheduleFrame\(bootstrapGeneration\);\s*\} else \{\s*running = false;/s);
  assert.equal(
    (bootstrap.match(/if \(stopped\) \{\s*createdRenderer\.free\?\.\(\);\s*return;/gs) ?? []).length,
    2,
    "stopping either delayed renderer creation path must dispose the unpublished renderer",
  );
});

test("transition state is observable without changing the presented mode", () => {
  const metrics = functionSlice("currentMetrics", "disposeRenderer");
  assert.match(metrics, /mode,/);
  assert.match(metrics, /transitionMode,/);
});

test("retained readiness publishes preload telemetry before playback ticks begin", () => {
  const bootstrap = functionSlice("bootstrapRenderer", "tryPresent");
  const initialPresent = bootstrap.indexOf("while (!tryPresent())");
  const ready = bootstrap.indexOf("const ready = {");
  const publish = bootstrap.indexOf("postMain({ type: \"ready\", ...ready });");
  const schedule = bootstrap.indexOf("scheduleFrame(bootstrapGeneration);");
  assert.ok(initialPresent >= 0 && ready > initialPresent, "ready requires the initial presentation");
  assert.ok(publish > ready && schedule > publish, "playback ticks must begin after ready publication");
  assert.match(bootstrap, /time: renderer\.time\(\),\s*presentedFrames,/s);
  assert.match(bootstrap, /ready\.preloadedGeometryCount = renderer\.preloadedGeometryCount\(\);/);
  assert.match(bootstrap, /ready\.preloadBytesUploaded = renderer\.preloadBytesUploaded\(\);/);

  const metrics = functionSlice("currentMetrics", "disposeRenderer");
  assert.match(metrics, /metrics\.preloadedGeometryCount = renderer\.preloadedGeometryCount\(\);/);
  assert.match(metrics, /metrics\.preloadBytesUploaded = renderer\.preloadBytesUploaded\(\);/);
});

test("stale frame callbacks cannot tick or spawn a second loop after transition", () => {
  const schedule = functionSlice("scheduleFrame", "frame");
  assert.match(schedule, /frame\(timestamp, generation, ticket\)/);
  const frame = functionSlice("frame", "drainGpuDiagnostics");
  const staleGuard = frame.indexOf("generation !== frameLoopGeneration");
  const tick = frame.indexOf('renderPort?.postMessage({ type: "tick", timestamp });');
  const reschedule = frame.indexOf("scheduleFrame(generation);");
  assert.ok(staleGuard >= 0 && tick > staleGuard && reschedule > tick);

  const begin = functionSlice("beginRendererTransition", "resize");
  assert.match(
    begin,
    /transitionFrameLoopWasRunning = running;\s*frameLoopGeneration \+= 1;\s*running = false;/s,
  );
  const stop = functionSlice("stop", "attachRenderPort");
  assert.match(stop, /stopped = true;\s*frameLoopGeneration \+= 1;\s*running = false;/s);
});
