// G4 authoring parity, using the existing real Rust/Pyodide pixel harness.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { createRuntimeBuildIdentity } from "./build-runtime-identity.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const identity = JSON.parse(await readFile(root + "/web/runtime-build-identity.json", "utf8"));
assert.deepEqual(await createRuntimeBuildIdentity(root), identity, "exact clean source/package required");
const source = await readFile(root + "/web/python/examples/glow_profile.py", "utf8");
await qualifyPairedAuthoring({
  artifactDirectory: "browser-smoke-artifacts/glow-profile",
  prepareContext: disableAuthoringJspi,
  cases: [1, 100].flatMap(active => ["intensity", "radius"].flatMap(property =>
    [0.5, 5, 9.5].map(sampleTime => ({
      id: `g4-${active}-${property}-${sampleTime}`,
      source: `context = ${JSON.stringify({ active_glows: active, property, duration: 10 })}\n${source}`,
      factory: "createDirectGlowProfileRenderer",
      factoryArgs: [active, property, 10],
      canvasSize: [1152, 648],
      duration: 10,
      sampleTime,
      objectCount: 601,
    })))),
});
