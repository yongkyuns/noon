import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const main = await readFile(new URL("./main.js", import.meta.url), "utf8");

assert.match(
  main,
  /let canvas = document\.querySelector\("#scene"\);/,
  "the playground must be able to adopt a replacement canvas from an unpublished prepared owner",
);

const createRuntimeStart = main.indexOf("function createRuntimeClient()");
const preparationStart = main.indexOf("function ensureRuntimePreparation()", createRuntimeStart);
const runtimeReadyStart = main.indexOf("async function ensureRuntimeReady({", preparationStart);
const executionReadyStart = main.indexOf("async function ensureExecutionReady()", runtimeReadyStart);
assert.ok(
  createRuntimeStart >= 0 &&
    preparationStart > createRuntimeStart &&
    runtimeReadyStart > preparationStart &&
    executionReadyStart > runtimeReadyStart,
  "prepared-owner creation and authored-engine startup boundaries must exist in order",
);

const createRuntimeBody = main.slice(createRuntimeStart, preparationStart);
const preparationBody = main.slice(preparationStart, runtimeReadyStart);
const runtimeReadyBody = main.slice(runtimeReadyStart, executionReadyStart);

assert.match(
  createRuntimeBody,
  /new AuthoringExecutionClient\(canvas,/,
  "first Run must create one execution facade candidate on demand",
);
assert.match(
  createRuntimeBody,
  /if \(player !== candidate\) return;/,
  "fatal callbacks from an unpublished candidate must not mark a public player for restart",
);
assert.match(
  preparationBody,
  /if \(runtimePreparation !== null\) return runtimePreparation;/,
  "Python or stale-run failures must be able to reuse an already prepared candidate",
);
assert.match(
  preparationBody,
  /const ready = candidate\.prepare\(\);/,
  "first Run must prepare the mode-free render owner",
);
assert.doesNotMatch(
  preparationBody,
  /candidate\.start(?:RetainedCanonical)?\(/,
  "mode-free preparation must not speculate a legacy or retained engine",
);
assert.match(
  preparationBody,
  /adoptRuntimeCanvas\(candidate\);[\s\S]*runtimePreparation = null;/,
  "failed render preparation must adopt the replacement canvas and release the failed candidate",
);

assert.match(
  runtimeReadyBody,
  /const prepared = preparation \?\? ensureRuntimePreparation\(\);/,
  "authored startup must consume the candidate prepared in parallel with Python",
);
assert.match(
  runtimeReadyBody,
  /await prepared\.ready;/,
  "engine attachment must wait for mode-free render preparation",
);
assert.match(
  runtimeReadyBody,
  /await nextPlayer\.startRetainedCanonical\(sceneSpecJson,/,
  "canonical retained first runs must attach directly after authoring selects retained mode",
);
assert.match(
  runtimeReadyBody,
  /await nextPlayer\.start\(sceneJson,/,
  "legacy first runs must attach directly after authoring selects legacy mode",
);
const retainedStart = runtimeReadyBody.indexOf("await nextPlayer.startRetainedCanonical(sceneSpecJson,");
const legacyStart = runtimeReadyBody.indexOf("await nextPlayer.start(sceneJson,");
const playerPublish = runtimeReadyBody.indexOf("player = nextPlayer;");
assert.ok(
  retainedStart >= 0 && legacyStart >= 0 && playerPublish > retainedStart && playerPublish > legacyStart,
  "the prepared candidate must remain unpublished until the selected authored engine is ready",
);
assert.match(
  runtimeReadyBody,
  /if \(runtimePreparation === prepared\) \{\s*runtimePreparation = null;/,
  "successful publication must consume the prepared candidate exactly once",
);
assert.match(
  runtimeReadyBody,
  /nextPlayer\.terminate\(\);\s*adoptRuntimeCanvas\(nextPlayer\);/,
  "failed engine attachment must terminate the candidate and adopt its fresh canvas",
);

const runSceneStart = main.indexOf("async function runScene()");
const selectExampleStart = main.indexOf("async function selectExample(", runSceneStart);
assert.ok(runSceneStart >= 0 && selectExampleStart > runSceneStart, "runScene boundary must exist");
const runSceneBody = main.slice(runSceneStart, selectExampleStart);
const preparationCall = runSceneBody.indexOf(
  "const preparation = player === null ? ensureRuntimePreparation() : null;",
);
const authoringCall = runSceneBody.indexOf("authored = await client.run(source,");
const runtimeCall = runSceneBody.indexOf("result = await ensureRuntimeReady({");
assert.ok(
  preparationCall >= 0 && preparationCall < authoringCall,
  "cold Run must kick render/WASM preparation before awaiting Python authoring",
);
assert.ok(
  runtimeCall > authoringCall,
  "engine selection and attachment must still wait for authored output",
);
assert.match(
  runSceneBody,
  /result = await ensureRuntimeReady\(\{\s*preparation,/,
  "cold authored startup must consume the preparation started before the Python await",
);

const bootStart = main.indexOf("try {\n  const requested = requestedExampleId();");
const bootCatch = main.indexOf("} catch (error) {\n  showError(error);", bootStart);
assert.ok(bootStart >= 0 && bootCatch > bootStart, "playground boot boundary must exist");
const bootBody = main.slice(bootStart, bootCatch);
assert.doesNotMatch(
  bootBody.slice(0, bootBody.indexOf("window.__noonExampleGallery")),
  /ensureRuntimePreparation\(|new AuthoringExecutionClient\(/,
  "page boot must remain fully deferred until an explicit Run",
);
assert.match(
  bootBody,
  /pagehide[\s\S]*const preparation = runtimePreparation;[\s\S]*runtimePreparation = null;[\s\S]*preparation\?\.candidate\.terminate\(\);[\s\S]*player\?\.terminate\(\);/,
  "page teardown must release an unpublished prepared owner before the published player",
);

console.log(
  "✓ first Run overlaps Python authoring with mode-free render preparation and publishes only the authored engine",
);
