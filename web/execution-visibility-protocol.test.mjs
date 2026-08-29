import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const engineUrl = new URL("./execution-engine-worker.js", import.meta.url);
const renderUrl = new URL("./authoring-render-worker.js", import.meta.url);

async function workerSources() {
  return Promise.all([readFile(engineUrl, "utf8"), readFile(renderUrl, "utf8")]);
}

test("legacy execution activates visibility before paired frame delivery", async () => {
  const [engine, render] = await workerSources();

  const activation = engine.indexOf('renderPort.postMessage({ type: "visibility_active" })');
  const tickDrain = engine.indexOf("drainWork();", activation);
  assert.ok(activation >= 0, "engine must acknowledge visibility activation");
  assert.ok(tickDrain > activation, "activation must be queued before tick work is drained");

  assert.match(engine, /const requestsVisibility = message\.aspect !== undefined && message\.aspect !== null/);
  assert.match(engine, /requestsVisibility && \(!Number\.isFinite\(message\.aspect\) \|\| message\.aspect <= 0\)/);
  assert.match(engine, /aspect: requestsVisibility \? message\.aspect : null/);
  assert.match(engine, /executionDeltaMetadata\(json\)/);
  assert.match(engine, /pendingVisibility = \{[\s\S]*?session: metadata\.session,[\s\S]*?sequence: metadata\.sequence/);
  assert.match(engine, /maxInFlight: 1/);
  assert.match(
    engine,
    /function handleTransportWritable\(\)[\s\S]*?sendVisibilityOrThrow\(pendingVisibility\)[\s\S]*?pendingVisibility = null/,
    "visibility must be published only after the paired execution delta is acknowledged",
  );
  assert.match(engine, /player\.viewportVisibilityJson\(viewportAspect\)/);

  assert.match(render, /message\.type === "visibility_active"/);
  assert.match(render, /message\.type === "visibility"/);
  assert.match(render, /renderer\.applyVisibilityJson\(message\.json\)/);
});

test("retained execution ticks omit aspect and bypass viewport visibility", async () => {
  const [engine, render] = await workerSources();

  assert.match(
    render,
    /else \{\s*renderPort\?\.postMessage\(\{ type: "tick", timestamp \}\);\s*\}/,
    "retained mode must request ordinary execution without activating viewport culling",
  );
  assert.match(
    engine,
    /else if \(tick\.aspect === null\) \{[\s\S]*?Ordinary execution ticks do not participate in viewport visibility culling/,
  );
});

test("renderer transitions drain transport without issuing premature engine ticks", async () => {
  const [, render] = await workerSources();

  const frameStart = render.indexOf("function frame(timestamp)");
  const transportDrain = render.indexOf("drainTransport();", frameStart);
  const transitionGate = render.indexOf(
    "transitionMode !== null || bootstrapPromise !== null || renderer === null",
    transportDrain,
  );
  const legacyTick = render.indexOf("if (mode === MODE_LEGACY)", transitionGate);
  assert.ok(transportDrain >= 0, "frame loop must continue draining replacement transport");
  assert.ok(transitionGate > transportDrain, "transition gate must run after transport draining");
  assert.ok(legacyTick > transitionGate, "no engine tick may be emitted before transition bootstrap settles");
  assert.match(
    render,
    /transitionMode !== null \|\| bootstrapPromise !== null \|\| renderer === null[\s\S]*?scheduleFrame\(\);[\s\S]*?return;/,
    "transition gating must keep the existing RAF loop alive without issuing work",
  );
});

test("legacy visibility is bound to the exact applied execution envelope", async () => {
  const [, render] = await workerSources();

  assert.match(render, /executionDeltaMetadata\(json\)/);
  assert.match(render, /executionMetadata = metadata/);
  assert.match(render, /message\.session !== executionMetadata\.session/);
  assert.match(render, /message\.sequence !== executionMetadata\.sequence/);
  assert.match(
    render,
    /executionMetadata = null;[\s\S]*?visibilityActive = false;/,
    "engine reconnect or transition must invalidate old execution pairing metadata",
  );
});

test("legacy no-change ticks preserve a mirror-compatible visibility frame", async () => {
  const [engine] = await workerSources();

  assert.match(
    engine,
    /lastVisibility !== null && Object\.is\(lastVisibility\.aspect, viewportAspect\)[\s\S]*?sendVisibilityOrThrow\(lastVisibility\)/,
    "unchanged ticks should reuse visibility already resolved against the current mirror frame",
  );
  assert.match(
    engine,
    /sendDeltaOrThrow\(player\.snapshotDeltaJson\(\)\)/,
    "first activation or an aspect change must synchronize the mirror before new visibility",
  );
});

test("legacy render ticks carry aspect and wait for matching visibility", async () => {
  const [, render] = await workerSources();

  assert.match(render, /aspect: width \/ height/);
  assert.match(
    render,
    /mode === MODE_LEGACY && visibilityActive && !visibilityReady[\s\S]*?return false;/,
  );
  assert.match(
    render,
    /!visibilityActivationPending && !awaitingVisibility && !needsPresent/,
  );
  assert.match(
    render,
    /visibilityActivationPending \|\| awaitingVisibility \|\| needsPresent/,
    "resize must defer while an old-aspect frame is pending",
  );
});
