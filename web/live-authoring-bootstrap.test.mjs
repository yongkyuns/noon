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
  /await afterInitialPaint\(\);[\s\S]*performance\.mark\("noon-live-authoring-preload-start"\);[\s\S]*await gallery\.run\(\);/,
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

// Execute the production bootstrap with only its module-loader boundary replaced.
// No polling implementation or source/runtime owner is recreated by this fixture.
const { default: vm } = await import("node:vm");
const { default: test } = await import("node:test");
assert.equal(bootstrap.split('import("./main.js")').length, 2);
assert.doesNotMatch(bootstrap, /API_WAIT_TIMEOUT_MS|API_POLL_MS|setTimeout|setInterval/,
  "readiness must follow the existing module, not a timed polling loop");

function fixture() {
  class Output { dataset = {}; }
  const status = new Output();
  let resolveMain, rejectMain, resolveRun, rejectRun, now = 0, imports = 0, runs = 0;
  const main = new Promise((resolve, reject) => { resolveMain = resolve; rejectMain = reject; });
  const run = new Promise((resolve, reject) => { resolveRun = resolve; rejectRun = reject; });
  const frames = [], warnings = [], marks = [], listeners = new Set();
  const window = {
    addEventListener(type, callback) { assert.equal(type, "pagehide"); listeners.add(callback); },
    removeEventListener(type, callback) { assert.equal(type, "pagehide"); listeners.delete(callback); },
  };
  const gallery = { selectedExampleId: "parity-animated-square-to-circle", run() { runs++; return run; } };
  vm.runInNewContext(bootstrap.replace('import("./main.js")', "loadMain()"), {
    document: { querySelector: () => status }, HTMLOutputElement: Output, window,
    loadMain: () => { imports++; return main; },
    performance: { now: () => now, mark: name => marks.push(name) },
    requestAnimationFrame: callback => { frames.push(callback); },
    setTimeout: () => { throw new Error("startup must not poll"); },
    console: { warn: (...args) => warnings.push(args) },
  });
  return {
    status, window, gallery, frames, warnings, marks, listeners,
    stats: () => ({ imports, runs }),
    elapse(milliseconds) { now += milliseconds; },
    resolveMain, rejectMain, resolveRun, rejectRun,
    pagehide() { for (const callback of listeners) callback({ persisted: true }); },
    frame() { assert.ok(frames.length, "expected pending animation frame"); frames.shift()(now); },
    async ready() { window.__noonExampleGallery = gallery; resolveMain(); await flush(); },
    async paint() { this.frame(); await flush(); this.frame(); await flush(); },
  };
}

async function flush() { await new Promise(resolve => setImmediate(resolve)); }

test("slow valid initialization outlives the former timeout and preloads once after paint", async () => {
  const f = fixture();
  // The regression trace needed ~21 seconds just to expose the initial API.
  f.elapse(60_000); await flush();
  assert.deepEqual(f.stats(), { imports: 1, runs: 0 });
  assert.equal(f.frames.length, 0);
  assert.deepEqual(f.warnings, []);
  await f.ready();
  assert.equal(f.status.dataset.liveAuthoring, "preloading");
  f.frame(); await flush();
  assert.equal(f.stats().runs, 0, "must retain the post-paint barrier");
  f.frame(); await flush();
  assert.equal(f.stats().runs, 1);
  assert.deepEqual(f.marks, ["noon-live-authoring-preload-start"]);
  f.resolveRun(); await flush();
  assert.equal(f.status.dataset.liveAuthoring, "ready");
  assert.equal(f.listeners.size, 0);
  assert.equal(f.frames.length, 0);
});

test("an already visible API still waits for its owning module evaluation", async () => {
  const f = fixture();
  f.window.__noonExampleGallery = f.gallery;
  await flush();
  assert.equal(f.frames.length, 0);
  assert.equal(f.stats().runs, 0);
  f.resolveMain(); await flush(); await f.paint();
  f.resolveRun(); await flush();
  assert.equal(f.stats().runs, 1);
});

test("navigation during module initialization cannot start a retired page", async () => {
  const f = fixture();
  assert.equal(f.listeners.size, 1, "retirement must be observed before readiness");
  f.pagehide(); await f.ready();
  assert.equal(f.stats().runs, 0);
  assert.equal(f.frames.length, 0);
  assert.equal(f.status.dataset.liveAuthoring, undefined);
  assert.deepEqual(f.warnings, []);
  assert.equal(f.listeners.size, 0);
});

test("navigation at the paint barrier cancels the preload", async () => {
  const f = fixture(); await f.ready();
  f.frame(); await flush(); f.pagehide(); f.frame(); await flush();
  assert.equal(f.stats().runs, 0);
  assert.deepEqual(f.marks, []);
  assert.equal(f.listeners.size, 0);
});

test("module failure retains its cause and does not attempt a run", async () => {
  const f = fixture(), error = new Error("manifest unavailable");
  f.rejectMain(error); await flush();
  assert.equal(f.status.dataset.liveAuthoring, "error");
  assert.equal(f.warnings.length, 1);
  assert.equal(f.warnings[0][1], error);
  assert.equal(f.stats().runs, 0);
  assert.equal(f.frames.length, 0);
  assert.equal(f.listeners.size, 0);
});

test("a handled initialization failure with no usable API fails without polling", async () => {
  const f = fixture();
  f.window.__noonExampleGallery = { selectedExampleId: "invalid" };
  f.resolveMain(); await flush();
  assert.equal(f.status.dataset.liveAuthoring, "error");
  assert.match(f.warnings[0][1].message, /API unavailable after initialization/);
  assert.equal(f.stats().runs, 0);
  assert.equal(f.frames.length, 0);
  assert.equal(f.listeners.size, 0);
});

test("run failure is reported once and never retried", async () => {
  const f = fixture(), error = new Error("source failed");
  await f.ready(); await f.paint(); f.rejectRun(error); await flush();
  assert.equal(f.status.dataset.liveAuthoring, "error");
  assert.equal(f.warnings.length, 1);
  assert.equal(f.warnings[0][1], error);
  assert.equal(f.stats().runs, 1);
  assert.equal(f.frames.length, 0);
  assert.equal(f.listeners.size, 0);
});

test("retired module and run completions cannot publish readiness or diagnostics", async () => {
  const loading = fixture(); loading.pagehide(); loading.rejectMain(new Error("late import")); await flush();
  assert.equal(loading.status.dataset.liveAuthoring, undefined);
  assert.deepEqual(loading.warnings, []);
  assert.equal(loading.listeners.size, 0);
  for (const fail of [false, true]) {
    const f = fixture(); await f.ready(); await f.paint(); f.pagehide();
    if (fail) f.rejectRun(new Error("retired run")); else f.resolveRun();
    await flush();
    assert.equal(f.status.dataset.liveAuthoring, "preloading");
    assert.deepEqual(f.warnings, []);
    assert.equal(f.listeners.size, 0);
  }
});

test("a reported authoring warmup failure remains an error", async () => {
  const f = fixture(); await f.ready(); await f.paint();
  f.status.dataset.authoringWarmup = "failed"; f.resolveRun(); await flush();
  assert.equal(f.status.dataset.liveAuthoring, "error");
  assert.equal(f.listeners.size, 0);
});
