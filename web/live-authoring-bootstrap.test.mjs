import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bootstrap = await readFile(new URL("./live-authoring-bootstrap.js", import.meta.url), "utf8");
const editorBootstrap = await readFile(new URL("./python-editor-bootstrap.js", import.meta.url), "utf8");

assert.match(
  editorBootstrap,
  /import\("\.\/live-authoring-bootstrap\.js"\)/,
  "playground bootstrap must start live-authoring preload without waiting for explicit Run",
);
assert.match(
  bootstrap,
  /await waitForGalleryApi\(\);/,
  "live authoring must reuse the initialized playground API instead of constructing a parallel client",
);
assert.match(
  bootstrap,
  /await afterInitialPaint\(\);[\s\S]*await gallery\.run\(\);/,
  "initial preload must use the initialized playground Run path after paint",
);
assert.doesNotMatch(
  bootstrap,
  /LatestSourceRunner|addEventListener\("input"|requestLatestSource/,
  "preload must not install a second editor lifecycle owner",
);
assert.match(
  bootstrap,
  /Subsequent edits are handled by main\.js[\s\S]*debounce only the next source submission/,
  "the bootstrap must leave edit cancellation and reruns to main.js",
);
assert.match(
  bootstrap,
  /function afterInitialPaint\(\)[\s\S]*requestAnimationFrame\(\(\) => \{[\s\S]*requestAnimationFrame\(\(\) => resolve\(\)\)/,
  "paint boundary helper must span two animation frames so one presentation opportunity occurs before preload",
);
assert.match(
  bootstrap,
  /status\.dataset\.liveAuthoring = "preloading"/,
  "browser diagnostics must expose live-authoring preload state",
);
assert.doesNotMatch(
  bootstrap,
  /new PythonAuthoringClient|new AuthoringExecutionClient|new Worker/,
  "live authoring must not introduce another Python client, execution owner, or worker topology",
);

console.log("✓ authoring preloads after paint without duplicating main.js edit lifecycle");
