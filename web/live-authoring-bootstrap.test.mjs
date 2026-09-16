import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bootstrap = await readFile(new URL("./live-authoring-bootstrap.js", import.meta.url), "utf8");
const editorBootstrap = await readFile(new URL("./python-editor-bootstrap.js", import.meta.url), "utf8");

assert.match(
  editorBootstrap,
  /import\("\.\/live-authoring-bootstrap\.js"\)/,
  "playground bootstrap must warm authoring without waiting for explicit Run",
);
assert.match(
  bootstrap,
  /const gallery = await waitForGalleryApi\(\);/,
  "authoring warmup must reuse the initialized playground API instead of constructing a parallel client",
);
assert.match(
  bootstrap,
  /await afterInitialPaint\(\);[\s\S]*await gallery\.run\(\);/,
  "initial preload must cross the explicit paint boundary before warming Python/runtime through the normal Run path",
);
assert.match(
  bootstrap,
  /function afterInitialPaint\(\)[\s\S]*requestAnimationFrame\(\(\) => \{[\s\S]*requestAnimationFrame\(\(\) => resolve\(\)\)/,
  "paint boundary helper must span two animation frames so one presentation opportunity occurs before preload",
);
assert.match(
  bootstrap,
  /status\.dataset\.liveAuthoring = "preloading"/,
  "browser diagnostics must expose authoring preload state",
);
assert.doesNotMatch(
  bootstrap,
  /LatestSourceRunner|addEventListener\("input"|requestLatestSource|runInFlight|currentExampleId/,
  "editing Python must not schedule, join, cancel, or otherwise mutate execution",
);
assert.doesNotMatch(
  bootstrap,
  /new PythonAuthoringClient|new AuthoringExecutionClient|new Worker/,
  "authoring warmup must not introduce another Python client, execution owner, or worker topology",
);

console.log("✓ authoring preloads after paint while Python edits remain execution-inert until Run");
