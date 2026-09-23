import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import test from "node:test";

const sourceUrl = new URL("../scripts/playground-gallery-selection-smoke.mjs", import.meta.url).href;
// Execute the real smoke orchestration with only browser/network/filesystem I/O
// replaced. These tests qualify artifact retention, not rendering or PNG codecs.
const source = (await readFile(new URL(sourceUrl), "utf8"))
  .replace(/^import .*;\n/gm, "")
  .replaceAll("import.meta.url", "sourceUrl");

async function runSmoke({ selectedPixels = 501, clearPixels = 0, captureFailure, writeFailure = false, deferWrites = false } = {}) {
  const names = ["baseline", "selected", "cleared"];
  const captures = names.map((name) => Buffer.from(name));
  const decoded = new Map(captures.map((bytes, index) => {
    const data = new Uint8Array(32 * 32 * 4);
    const changed = [0, selectedPixels, clearPixels][index];
    for (let i = 0; i < changed; i += 1) data[i * 4] = 1;
    return [bytes, { width: 32, height: 32, data }];
  }));
  const events = [];
  const writes = new Map();
  let frame = 0;
  let capture = 0;
  const canvas = {
    boundingBox: async () => ({ x: 0, y: 0, width: 800, height: 600 }),
    screenshot: async () => {
      const name = names[capture];
      events.push(`capture:${name}`);
      if (name === captureFailure) throw new Error(`capture failed: ${name}`);
      return captures[capture++];
    },
  };
  const page = {
    setDefaultTimeout() {}, on() {}, goto: async () => {},
    evaluate: async (fn) => fn(),
    waitForFunction: async (fn, argument) => assert.ok(await fn(argument)),
    locator: () => canvas,
    mouse: { click: async () => { events.push("click"); frame += 1; } },
  };
  let error;
  try {
    await vm.runInNewContext(`(async () => {${source}\n})()`, {
      assert, path, fileURLToPath, sourceUrl, URL,
      process: { env: {} },
      spawn: () => ({ kill: () => events.push("server:kill") }),
      mkdir: async () => {},
      writeFile: async (filename, bytes) => {
        const name = path.basename(filename);
        events.push(`write:${name}`);
        if (writeFailure === true || writeFailure === name) throw new Error("artifact write failed");
        if (deferWrites) await new Promise(setImmediate);
        writes.set(name, bytes);
        events.push(`stored:${name}`);
      },
      fetch: async () => ({ ok: true, text: async () => "worker" }),
      PNG: { sync: { read: (bytes) => decoded.get(bytes) } },
      playwright: { chromium: { launch: async () => ({
        newContext: async () => ({ newPage: async () => page }),
        close: async () => events.push("browser:close"),
      }) } },
      createPyodideResourceCache: () => ({ install: async () => {} }),
      window: { __noonExampleGallery: {
        selectedExampleId: "noon-pointer-selection", run: async () => {}, runInFlight: false,
        executionMetrics: async () => ({ metrics: { presentedFrames: frame } }),
      } },
      document: { querySelector: () => ({ dataset: {
        state: "applied", interaction: "pointer-fill-selection", rendererBackend: "test-only",
      } }) },
      console: { log: () => events.push("passed"), error: () => events.push("diagnostic:error") },
    });
  } catch (caught) { error = caught; }
  return { error, writes, events, captures };
}

function assertRetained(result, names) {
  for (const name of names) {
    const index = ["baseline", "selected", "cleared"].indexOf(name);
    assert.equal(result.writes.get(`${name}.png`), result.captures[index]);
  }
  assert.equal(result.events.filter((event) => event === "browser:close").length, 1);
  assert.equal(result.events.filter((event) => event === "server:kill").length, 1);
  // Artifact I/O must not insert a wait before any original screenshot.
  assert.ok(result.events.findIndex((event) => event.startsWith("write:")) >
    result.events.findLastIndex((event) => event.startsWith("capture:")));
  return JSON.parse(result.writes.get("result.json"));
}

test("successful exact clear retains original captures and measurements", async () => {
  const result = await runSmoke();
  assert.equal(result.error, undefined);
  const report = assertRetained(result, ["baseline", "selected", "cleared"]);
  assert.equal(report.selectedChanged, 501);
  assert.equal(report.clearDifference, 0);
  assert.equal(report.error, null);
  assert.equal(result.events.at(-1), "passed");
});

test("21 differing pixels still fail but retain all original evidence", async () => {
  const result = await runSmoke({ clearPixels: 21 });
  assert.equal(result.error?.code, "ERR_ASSERTION");
  assert.equal(result.error.actual, 21);
  assert.equal(result.error.expected, 0);
  const report = assertRetained(result, ["baseline", "selected", "cleared"]);
  assert.equal(report.clearDifference, 21);
  assert.match(report.error, /background clear must restore/);
  assert.ok(!result.events.includes("passed"));
});

test("insufficient selection preserves only the captures actually taken", async () => {
  const result = await runSmoke({ selectedPixels: 500 });
  assert.match(result.error?.message, /selection changed only 500 pixels/);
  const report = assertRetained(result, ["baseline", "selected"]);
  assert.equal(report.selectedChanged, 500);
  assert.equal(report.clearDifference, undefined);
  assert.ok(!result.writes.has("cleared.png"));
  assert.equal(result.events.filter((event) => event === "click").length, 1);
});

test("a later screenshot failure cannot discard earlier captures", async () => {
  const result = await runSmoke({ captureFailure: "cleared" });
  assert.match(result.error?.message, /capture failed: cleared/);
  const report = assertRetained(result, ["baseline", "selected"]);
  assert.match(report.error, /capture failed: cleared/);
  assert.ok(!result.writes.has("cleared.png"));
});

test("artifact write failure never replaces the original strict assertion", async () => {
  const result = await runSmoke({ clearPixels: 21, writeFailure: true });
  assert.equal(result.error?.code, "ERR_ASSERTION");
  assert.equal(result.error.actual, 21);
  assert.ok(result.events.includes("diagnostic:error"));
  assert.ok(result.events.includes("browser:close"));
  assert.ok(result.events.includes("server:kill"));
  assert.ok(!result.events.includes("passed"));
});

test("artifact write failure after passing pixels still fails the smoke", async () => {
  const result = await runSmoke({ writeFailure: true });
  assert.match(result.error?.message, /artifact write failed/);
  assert.ok(result.events.includes("browser:close"));
  assert.ok(result.events.includes("server:kill"));
  assert.ok(!result.events.includes("passed"));
});

test("one failed write cannot abandon other pending capture writes", async () => {
  const result = await runSmoke({ clearPixels: 21, writeFailure: "result.json", deferWrites: true });
  assert.equal(result.error?.actual, 21);
  assert.equal(result.writes.size, 3);
  assert.ok(result.events.findLastIndex((event) => event.startsWith("stored:")) <
    result.events.indexOf("browser:close"));
});
