import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const bootstrap = await readFile(new URL("./live-authoring-bootstrap.js", import.meta.url), "utf8");
const editorBootstrap = await readFile(new URL("./python-editor-bootstrap.js", import.meta.url), "utf8");
const runner = await readFile(new URL("./latest-source-runner.js", import.meta.url), "utf8");

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
  /run: \(\) => gallery\.run\(\)/,
  "live edits must use the existing full-source Run path",
);
assert.match(
  bootstrap,
  /runInFlight: \(\) => gallery\.runInFlight/,
  "live edits must observe the existing in-flight Run boundary",
);
assert.match(
  bootstrap,
  /currentExampleId: \(\) => gallery\.selectedExampleId/,
  "live edits must stay pinned to the currently selected example",
);
assert.match(
  bootstrap,
  /editor\.addEventListener\("input", onInput\)/,
  "editor input must schedule a debounced full-source rerun",
);
assert.match(
  bootstrap,
  /await afterInitialPaint\(\);[\s\S]*requestLatestSource\(\{ immediate: true \}\)/,
  "initial preload must cross the explicit paint boundary before warming Python/runtime",
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

assert.match(
  runner,
  /const joinedExistingRun = Boolean\(this\.#runInFlight\(\)\);[\s\S]*await this\.#run\(\);[\s\S]*if \(joinedExistingRun\) \{\s*continue;/,
  "an edit that arrives during an older Run must issue a fresh rerun after that Run completes",
);
assert.match(
  runner,
  /targetExampleId !== this\.#currentExampleId\(\)/,
  "queued edits must not leak across example selection changes",
);
assert.doesNotMatch(
  runner,
  /PythonAuthoringClient|AuthoringExecutionClient|ExecutionWorkerClient|SemanticMutation|SceneRevision|ExecutionRevision|FrameEpoch/,
  "editor request coalescing must remain outside engine semantic/publication authority",
);

console.log("✓ live authoring preloads one existing session after paint and coalesces edits onto full-source Run");
