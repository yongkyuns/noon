import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

test("authoring tools cannot restore the deleted frontend identity authority", async () => {
  for (const file of ["authoring-perf.js", "main.js", "scene-perf.js"]) {
    const source = await readFile(new URL(file, import.meta.url), "utf8");
    assert.doesNotMatch(source, /SceneIdentityMap|NoonCanvasPlayer|\.reconcileScene\(/, file);
  }
  for (const file of ["scene-identity.js", "scene-pipeline-perf.mjs"]) {
    await assert.rejects(access(new URL(file, import.meta.url)), { code: "ENOENT" });
  }
});

test("scene performance reports worker provenance and backing resolution without extra sampling", async () => {
  const source = await readFile(new URL("scene-perf.js", import.meta.url), "utf8");
  assert.match(source, /import \{ ProvenancedPythonAuthoringClient \} from "\.\/provenanced-authoring-client\.js"/);
  assert.doesNotMatch(source, /import \{ PythonAuthoringClient \} from "\.\/authoring-client\.js"/);
  assert.match(source, /client = new ProvenancedPythonAuthoringClient\(\)/);
  assert.match(source, /runtimeBuildIdentity = readyIdentity/);
  assert.match(source, /\.\.\(runtimeBuildIdentity === null \? \{\} : \{ runtimeBuild: runtimeBuildIdentity \}\)/);
  assert.match(source, /backingResolution: \[canvas\.width, canvas\.height\]/);
  assert.match(source, /const rendererSamples = parameters\.get\("includeRendererSamples"\) === "1" \? \[\] : null/);
  assert.match(source, /profilePublicationStages: rendererSamples !== null/);
});
