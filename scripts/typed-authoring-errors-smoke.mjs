// Real Rust/WASM -> JavaScript -> pinned Pyodide qualification for #1292 R3.
// Fixtures submit public typed operations. Diagnostic snapshots are observations
// only: no snapshot is replayed into an engine, and no message is classified.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = 4198;
const origin = `http://127.0.0.1:${port}`;
const artifacts = path.resolve(process.env.NOON_TYPED_ERRORS_ARTIFACTS ?? path.join(root, "browser-smoke-artifacts/typed-authoring-errors"));
const worker = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
// Use the product's actual pin; this test must not silently qualify another Python runtime.
const pyodideUrl = worker.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
assert.ok(pyodideUrl, "the product worker must identify its pinned Pyodide module");
const pythonMapper = await readFile(path.join(root, "web/python/_noon_errors.py"), "utf8");
await mkdir(artifacts, { recursive: true });

let output = "";
const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root], { cwd: root, stdio: ["ignore", "pipe", "pipe"] });
server.stdout.on("data", chunk => { output = (output + chunk).slice(-64_000); });
server.stderr.on("data", chunk => { output = (output + chunk).slice(-64_000); });
let serverError;
const serverExit = new Promise(resolve => {
  server.once("exit", resolve);
  server.once("error", error => { serverError = error; resolve(); });
});
let browser;
const consoleMessages = [];
const report = { pyodideUrl, javascript: [], python: null, publicPython: null };
try {
  for (let attempt = 0; ; attempt++) {
    try {
      const response = await fetch(`${origin}/AGENTS.md`);
      if (response.ok) break;
    } catch { /* startup only; test errors are never swallowed */ }
    if (attempt === 79 || serverError || server.exitCode !== null) throw new Error(`test server failed to start\n${serverError ?? ""}\n${output}`);
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  browser = await chromium.launch({ channel: "chromium", headless: true, args: ["--disable-dev-shm-usage"] });
  const page = await browser.newPage();
  page.setDefaultTimeout(60_000);
  page.on("console", message => consoleMessages.push(`${message.type()}: ${message.text()}`));
  page.on("pageerror", error => consoleMessages.push(`pageerror: ${error.stack ?? error}`));
  // An inert same-origin document: no playground, renderer, or second authoring
  // worker is needed to prove this language boundary.
  await page.goto(`${origin}/AGENTS.md`);
  await page.evaluate(async () => {
    const wasm = await import("/web/pkg/noon_web.js");
    await wasm.default();
    const check = (value, message) => { if (!value) throw new Error(message); };
    const equal = (actual, expected, message) => check(JSON.stringify(actual) === JSON.stringify(expected), message);
    const key = handle => `${handle.semanticSlot}:${handle.semanticGeneration}`;
    const batch = (kind, pairs) => {
      const value = new wasm.WasmSceneMembershipBatch(kind);
      for (const [id, handle] of pairs) {
        value.reserveMobjectBinding(String(id), handle);
        value.appendMobject(String(id), handle);
      }
      return value;
    };
    const family = (store, handle) => {
      const members = new wasm.WasmSceneMembershipBatch("add");
      members.appendMobject("", handle);
      return store.createFamily(members);
    };
    const snapshot = (context, live) => ({
      members: Array.from(context.rootMembershipKeys()),
      duration: context.authoredDuration(),
      handoff: context.liveHandoffDuration() ?? null,
      ownership: context.liveExecutionOwnership(),
      frame: live ? context.liveDebugFrameJson() : null,
    });
    const describe = error => ({
      category: error.category,
      code: error.code,
      operation: error.operation,
      message: error.message,
      cause: error.cause ? describe(error.cause) : null,
    });
    const requireFailure = (invoke, category) => {
      let rejected;
      try { invoke(); } catch (error) { rejected = error; }
      check(rejected !== undefined, "invalid operation unexpectedly succeeded");
      check(rejected instanceof Error, "the WASM rejection must survive Pyodide as a JavaScript Error");
      check(rejected.noonErrorVersion === 1, "the failure must carry the shared projection version");
      check(rejected.category === category, `wrong failure category: ${rejected.category}`);
      check(typeof rejected.code === "string" && rejected.code.length > 0, "missing machine-readable code");
      check(typeof rejected.operation === "string" && rejected.operation.length > 0, "missing operation diagnostic");
      check(typeof rejected.message === "string" && rejected.message.length > 0, "missing human-readable diagnostic");
      return rejected;
    };

    const membershipFixture = (kind, live = false) => {
      const store = new wasm.WasmAuthoringStore();
      const otherStore = new wasm.WasmAuthoringStore();
      const context = store.createSceneContext();
      const otherContext = otherStore.createSceneContext();
      const first = store.createManimCircle(0.5);
      const foreign = otherStore.createManimCircle(0.5);
      check(key(first) === key(foreign), "fixture must exercise equal numeric IDs in different stores");
      const next = store.createManimSquare(0.5);
      const recovery = [store.createManimCircle(0.2), store.createManimSquare(0.2)];
      const families = [];
      if (kind === "ambiguous") {
        families.push(family(store, first), family(store, first));
        const initial = new wasm.WasmSceneMembershipBatch("add");
        initial.reserveMobjectBinding("0", first);
        for (const value of families) initial.appendFamily(value);
        context.editMembership(initial);
      }
      if (kind === "pending") {
        context.beginOrdinaryWait(0.25);
        live = true;
      } else if (live) {
        context.beginLiveExecution(1.0);
      }
      const before = snapshot(context, live);
      return {
        kind,
        live,
        reject: () => {
          const replacing = kind === "missing" || kind === "ambiguous";
          context.editMembership(batch(replacing ? "replace" : "add", [[0, first], [1, kind === "foreign" ? foreign : next]]));
        },
        assertAtomic: () => equal(snapshot(context, live), before, `${kind}: rejected operation changed coherent state`),
        recover: () => {
          if (kind === "pending") {
            const player = context.createExecutionPlayer(0.25, 73);
            const drive = player.driveLiveSegmentToAuthoredTime(0.25);
            check(drive.reachedEndpoint, "pending continuation did not reach its endpoint");
            drive.free();
            player.completeLiveSegment();
            context.returnExecutionPlayer(player);
          }
          if (kind === "ambiguous") {
            context.editMembership(new wasm.WasmSceneMembershipBatch("clear"));
            // Binding 0 already belonged to first before rejection; binding 1
            // was only a reservation and must be reusable for a different node.
            context.editMembership(batch("add", [[0, first], [1, recovery[1]]]));
            equal(Array.from(context.rootMembershipKeys()), [key(first), key(recovery[1])], "ambiguous replacement recovery failed");
          } else {
            // Reuse BOTH rejected wrapper IDs for DIFFERENT local nodes. A
            // leaked preflight reservation would reject this successful add.
            context.editMembership(batch("add", [[0, recovery[0]], [1, recovery[1]]]));
            equal(Array.from(context.rootMembershipKeys()), recovery.map(key), `${kind}: failed reservations leaked`);
          }
          if (kind === "pending") {
            check(JSON.parse(context.liveDebugFrameJson()).time === 0.25, "recovery restarted instead of continuing the same timeline");
          }
          return snapshot(context, live);
        },
        dispose: () => {
          context.free(); otherContext.free();
          for (const value of [...families, first, foreign, next, ...recovery]) value.free();
          store.free(); otherStore.free();
        },
      };
    };

    const ownershipFixture = index => {
      const store = new wasm.WasmAuthoringStore();
      const foreignStore = new wasm.WasmAuthoringStore();
      const contexts = [store.createSceneContext(), store.createSceneContext(), foreignStore.createSceneContext()];
      const players = contexts.map(context => context.createExecutionPlayer(1, 41));
      const frames = players.map(player => player.debugFrameJson());
      const resources = players.map(player => Array.from(player.resourceBundleBytes()));
      const phases = () => contexts.map(context => context.liveExecutionOwnership());
      const before = phases();
      return {
        reject: () => contexts[0].returnExecutionPlayer(players[index]),
        assertAtomic: () => equal(phases(), before, "rejected return changed player ownership"),
        recover: player => {
          players[index] = player;
          equal(players.map(value => value.debugFrameJson()), frames, "takePlayer rebuilt or changed the runtime");
          equal(players.map(value => Array.from(value.resourceBundleBytes())), resources, "takePlayer changed retained resources");
          const sessions = players.map(value => JSON.parse(value.initialDeltaJson()).session);
          equal(sessions, [41, 41, 41], "takePlayer changed the original transport sessions");
          contexts.forEach((context, i) => context.returnExecutionPlayer(players[i]));
          equal(phases(), ["returned", "returned", "returned"], "recovered players could not return to their rightful owners");
          return phases();
        },
        dispose: () => { for (const context of contexts) context.free(); store.free(); foreignStore.free(); },
      };
    };
    window.noonTypedErrorFixtures = { membershipFixture, ownershipFixture, describe, requireFailure };
  });

  report.javascript = await page.evaluate(() => {
    const { membershipFixture, ownershipFixture, describe, requireFailure } = window.noonTypedErrorFixtures;
    const results = [];
    for (const [kind, live, category] of [
      ["foreign", false, "foreign_handle"], ["missing", false, "invalid_input"],
      ["ambiguous", false, "invalid_input"], ["foreign", true, "foreign_handle"],
      ["missing", true, "invalid_input"], ["pending", true, "pending"],
    ]) {
      const fixture = membershipFixture(kind, live);
      try {
        const error = requireFailure(fixture.reject, category);
        fixture.assertAtomic();
        const recovered = fixture.recover();
        results.push({ kind, live, error: describe(error), recovered });
      } finally { fixture.dispose(); }
    }
    for (const index of [1, 2]) {
      const fixture = ownershipFixture(index);
      try {
        const error = requireFailure(fixture.reject, "ownership");
        fixture.assertAtomic();
        const diagnostic = describe(error);
        const recovered = fixture.recover(error.takePlayer());
        results.push({ kind: index === 1 ? "foreign_root" : "foreign_store", error: diagnostic, recovered });
      } finally { fixture.dispose(); }
    }
    return results;
  });
  assert.equal(report.javascript.length, 8);
  for (const row of report.javascript.filter(row => row.kind === "missing" || row.kind === "ambiguous")) {
    assert.ok(row.error.cause, `${row.kind}: semantic cause was flattened at the language boundary`);
  }

  report.python = await page.evaluate(async ({ pyodideUrl, pythonMapper }) => {
    const { loadPyodide } = await import(pyodideUrl);
    const pyodide = await loadPyodide();
    pyodide.FS.writeFile("/tmp/_noon_errors.py", pythonMapper);
    // Normal foreign JS errors must not acquire a Noon category based on words.
    window.noonUnmarkedError = () => { throw new Error("foreign handle invalid membership unsupported stale pending"); };
    return JSON.parse(pyodide.runPython(`
import json, sys
sys.path.insert(0, "/tmp")
from _noon_errors import call_engine
from js import noonTypedErrorFixtures as fixtures, noonUnmarkedError
from pyodide.ffi import JsException

results = []
for kind, live, category, exception_type in [
    ("foreign", False, "foreign_handle", ValueError),
    ("missing", False, "invalid_input", ValueError),
    ("ambiguous", False, "invalid_input", ValueError),
    ("foreign", True, "foreign_handle", ValueError),
    ("missing", True, "invalid_input", ValueError),
    ("pending", True, "pending", RuntimeError),
]:
    fixture = fixtures.membershipFixture(kind, live)
    try:
        try:
            call_engine(fixture.reject)
        except exception_type as error:
            assert error.category == category, (kind, error.category)
            assert error.code and error.operation and str(error)
            assert error.__cause__ is not None
            if kind in ("missing", "ambiguous"):
                assert error.cause is not None, "Rust semantic cause was flattened"
                assert error.cause.code and str(error.cause)
            fixture.assertAtomic()
            fixture.recover()
            results.append({"kind": kind, "live": live, "category": error.category, "code": error.code})
        else:
            raise AssertionError("invalid membership operation succeeded")
    finally:
        fixture.dispose()

for index in (1, 2):
    fixture = fixtures.ownershipFixture(index)
    try:
        try:
            call_engine(fixture.reject)
        except RuntimeError as error:
            assert error.category == "ownership"
            assert error.code and error.operation and str(error)
            assert callable(error.takePlayer), "mapped exception lost rejected-player ownership"
            fixture.assertAtomic()
            fixture.recover(error.takePlayer())
            results.append({"kind": "ownership", "foreign_index": index, "category": error.category, "code": error.code})
        else:
            raise AssertionError("foreign ownership return succeeded")
    finally:
        fixture.dispose()

try:
    call_engine(noonUnmarkedError)
except JsException as error:
    assert not hasattr(error, "category"), "mapper guessed a category from diagnostic words"
else:
    raise AssertionError("unmarked JS error was swallowed")
json.dumps(results)
`));
  }, { pyodideUrl, pythonMapper });
  assert.equal(report.python.length, 8);

  // The public Python Scene callsites must actually use the mapper, not merely
  // leave a correct but unused helper beside the old ValueError(str(...)) path.
  report.publicPython = await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("/web/authoring-client.js");
    const client = new PythonAuthoringClient();
    try {
      await client.ready();
      const result = await client.run(`
from noon import Scene, Circle, Square
scene = Scene()
anchor = Circle(radius=0.3)
scene.add(anchor)
missing = Circle(radius=0.2)
replacement = Square(side_length=0.2)
before_members = list(scene.mobjects)
before_id = scene._next_object_id
try:
    scene.replace(missing, replacement)
except ValueError as error:
    assert error.category == "invalid_input"
    assert error.code and error.operation and str(error)
    assert error.cause is not None and error.cause.code
else:
    raise AssertionError("Scene.replace accepted a missing member")
assert list(scene.mobjects) == before_members
assert scene._next_object_id == before_id
assert missing._scene is None and replacement._scene is None
scene.add(replacement)
assert list(scene.mobjects) == [anchor, replacement]
result = scene
`);
      if (!result.semanticExecution || client.terminated) throw new Error("failed Scene operation poisoned the authoring worker");
      const recovered = await client.run("from noon import Scene, Circle\nresult = Scene()\nresult.add(Circle(radius=0.2))");
      return { structuredScene: Boolean(result.semanticExecution), subsequentRun: Boolean(recovered.semanticExecution), terminated: client.terminated };
    } finally { client.terminate(); }
  });
  assert.deepEqual(report.publicPython, { structuredScene: true, subsequentRun: true, terminated: false });
  await writeFile(path.join(artifacts, "results.json"), JSON.stringify(report, null, 2));
  console.log(`Typed authoring boundary: ${report.javascript.length} WASM + ${report.python.length} Pyodide cases, unmarked-error negative control, public Scene callsite and worker recovery passed.`);
} catch (error) {
  await writeFile(path.join(artifacts, "failure.txt"), error.stack ?? String(error));
  await writeFile(path.join(artifacts, "partial-results.json"), JSON.stringify(report, null, 2));
  throw error;
} finally {
  await browser?.close();
  server.kill("SIGTERM");
  await serverExit;
  await writeFile(path.join(artifacts, "server.log"), output);
  await writeFile(path.join(artifacts, "browser.log"), consoleMessages.join("\n"));
}
