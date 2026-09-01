import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

const smoke = await readFile(
  new URL("../scripts/retained-execution-worker-smoke.mjs", import.meta.url),
  "utf8",
);
const generalClient = await readFile(new URL("./execution-worker-client.js", import.meta.url), "utf8");
const renderEntry = await readFile(new URL("./execution-render-worker.js", import.meta.url), "utf8");

await assert.rejects(
  access(new URL("./retained-execution-worker-client.js", import.meta.url)),
  (error) => error?.code === "ENOENT",
  "the standalone retained execution client must stay retired",
);
await assert.rejects(
  access(new URL("./retained-execution-render-worker.js", import.meta.url)),
  (error) => error?.code === "ENOENT",
  "retained execution must not regain a second render owner",
);

assert.match(
  smoke,
  /import\("\.\/execution-worker-client\.js"\)/,
  "retained browser qualification must exercise the shared execution client",
);
assert.match(
  smoke,
  /startRetainedCanonical\(sceneSpecJson,/,
  "retained browser qualification must start from canonical SceneSpec",
);
assert.doesNotMatch(
  smoke,
  /RetainedExecutionWorkerClient|runCompatibilityFallback|\.start\(sceneJson, retainedDocumentJson,/,
  "retained browser qualification must not preserve the retired split-player path",
);
assert.match(
  generalClient,
  /new URL\("\.\/retained-execution-engine-worker\.js", import\.meta\.url\)/,
  "the shared execution client remains the retained engine owner",
);
assert.match(
  renderEntry,
  /import "\.\/authoring-render-worker\.js";/,
  "legacy and retained execution must share the permanent authoring render owner",
);

console.log("✓ retained browser execution has one client and one render-owner topology");
