import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import { waitForBrowserObservation } from "../scripts/playground-browser-support.mjs";
import { assertNonreplayableShowcase, readLiveState } from "../scripts/showcase-live-review.mjs";
import { normalizeShowcaseManifest } from "./showcase-gallery.js";
import test from "node:test";

const sourceUrl = new URL("../scripts/playground-gallery-selection-smoke.mjs", import.meta.url).href;
const showcaseManifest = JSON.parse(await readFile(new URL("./python/examples/noon_showcase_manifest.json", import.meta.url), "utf8"));
// Execute the real smoke orchestration with only browser/network/filesystem I/O
// replaced. These tests qualify artifact retention, not rendering or PNG codecs.
const source = (await readFile(new URL(sourceUrl), "utf8"))
  .replace(/^import .*;\n/gm, "")
  .replaceAll("import.meta.url", "sourceUrl");

async function runSmoke({ selectedPixels = 501, clearPixels = 0, authoredPixels = 501, captureFailure, writeFailure = false, deferWrites = false, wheelReplies = [], env = {} } = {}) {
  const names = [
    "baseline", "selected", "cleared",
    "authoredBaseline", "indicated", "restored", "repeated", "repeatedRestored", "background",
    "zoomed", "zoomIndicated", "zoomRestored", "zoomReset",
    "nativeDragBaseline", "nativeDragReleased", "nativeDragHover", "nativeDragCancelTransient",
    "nativeDragCancelled", "nativeDragCancelSettled", "nativeDragRunReset",
  ];
  const captures = names.map((name) => Buffer.from(name));
  const decoded = new Map(captures.map((bytes, index) => {
    const data = new Uint8Array(64 * 64 * 4);
    const changed = [0, selectedPixels, clearPixels, 0, authoredPixels, 0, authoredPixels, 0, 0,
      1000, 400, 1000, 0, 0, 650, 650, 1400, 650, 650, 0][index];
    for (let i = 0; i < changed; i += 1) data[i * 4] = 1;
    return [bytes, { width: 64, height: 64, data }];
  }));
  const events = [];
  let contextOptions;
  let launchOptions;
  const layoutSizes = [];
  const writes = new Map();
  let frame = 0;
  let capture = 0;
  let showcase = false;
  const gallery = {
    selectedExampleId: "noon-pointer-selection", run: async () => {}, runInFlight: false,
    executionMetrics: async () => ({ metrics: { presentedFrames: frame, objectCount: 3 } }),
  };
  const browserWindow = { __noonNoJspiWorkerWrapped: true, __noonExampleGallery: gallery };
  const browserDocument = { querySelector: (selector) => {
    if (selector === ".playback-controls") return {
      dataset: { busy: "false", elapsedSeconds: "2.6", controllable: "false", playing: "false" },
      title: "Replay unavailable: UnsupportedDomain",
      querySelector: () => ({ getAttribute: () => "Play animation", disabled: true }),
    };
    if ([".playback-toggle", ".playback-restart", ".playback-scrubber"].includes(selector)) return { disabled: true };
    if (selector === "#patch-status") return { dataset: { state: "applied" }, value: "" };
    if (selector === "#status-text") return { textContent: "" };
    return { dataset: {
      state: "applied", interaction: showcase ? "none" : "pointer-fill-selection", rendererBackend: "WebGL2",
    } };
  } };
  const inspection = { pending: [], samples: [] };
  browserWindow.__noonInspectionTest = inspection;
  const acceptWheel = () => {
    const changed = wheelReplies.length ? wheelReplies.shift() : true;
    if (changed) frame += 1;
    inspection.pending.push(Promise.resolve({ inspectionScrollChanged: changed }));
  };
  const canvas = {
    evaluate: async (fn, argument) => {
      if (String(fn).includes("new PointerEvent")) {
        frame += 1;
        return true;
      }
      return fn({ style: { setProperty() {} },
        getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
        dispatchEvent: event => { event.defaultPrevented = true; acceptWheel(); events.push("DOM:wheel"); },
      }, argument);
    },
    boundingBox: async () => ({ x: 0, y: 0, width: 800, height: 600 }),
    screenshot: async () => {
      const name = names[capture];
      if (!name) throw new Error(`unexpected screenshot read after ${capture} planned captures`);
      events.push(`capture:${name}`);
      if (name === captureFailure) throw new Error(`capture failed: ${name}`);
      return captures[capture++];
    },
  };
  const page = {
    setDefaultTimeout() {}, on() {},
    goto: async (url) => {
      showcase = String(url).includes("catalog=showcase");
      gallery.selectedExampleId = showcase
        ? String(url).match(/example=([^&]+)/)?.[1] ?? "showcase-pointer-selection"
        : "noon-pointer-selection";
    },
    evaluate: async (fn, argument) => {
      if (String(fn).includes("await import(")) return undefined;
      const value = await vm.runInNewContext(`(${fn.toString()})(argument)`, {
        window: browserWindow, document: browserDocument, argument,
      });
      return value === undefined ? value : JSON.parse(JSON.stringify(value));
    },
    waitForFunction: async (fn, argument) => assert.ok(await fn(argument)),
    waitForTimeout: async () => {},
    locator: (selector) => {
      if (selector === "#scene") return canvas;
      if (selector === "#patch-status") return { getAttribute: async () => "applied" };
      if (selector === "#status") return { getAttribute: async () => "WebGL2" };
      if (selector === "#replace-scene") return { click: async () => events.push("public:run") };
      if (selector === ".playback-scrubber") return {
        getAttribute: async () => "2.6",
        evaluate: async (fn, value) => fn({ value: "", dispatchEvent() {} }, value),
      };
      return canvas;
    },
    getByRole: () => ({ count: async () => 1, click: async () => {} }),
    mouse: { move: async () => {}, wheel: async () => { acceptWheel(); }, click: async (x, y) => {
      events.push(`mouse:${x}:${y}`);
      if (!showcase || x > 100) frame += 1;
    } },
    touchscreen: { tap: async (x, y) => {
      events.push(`touch:${x}:${y}`);
      if (!showcase || x > 100) frame += 1;
    } },
  };
  let error;
  try {
    await vm.runInNewContext(`(async () => {${source}\n})()`, {
      assert, path, fileURLToPath, sourceUrl, URL,
      assertNonreplayableShowcase, readLiveState, normalizeShowcaseManifest,
      process: { env },
      setTimeout: (callback) => { callback(); return 0; },
      clearTimeout: () => {},
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
      fetch: async (url) => ({
        ok: true,
        text: async () => String(url).includes("showcase_pointer_selection.py")
          ? "self.on_click(circle, Indicate(circle))" : "worker",
        json: async () => showcaseManifest,
      }),
      PNG: { sync: { read: (bytes) => decoded.get(bytes) } },
      playwright: Object.fromEntries(["chromium", "firefox", "webkit"].map((name) => [name, { launch: async (options) => {
        launchOptions = options;
        return {
          newContext: async (options) => { contextOptions = options; return { newPage: async () => page }; },
          close: async () => events.push("browser:close"),
        };
      } }])),
      disableAuthoringJspi: async () => {},
      waitForBrowserObservation,
      playgroundLaunchOptions: (name) => ({ browser: name, headless: true }),
      createPyodideResourceCache: () => ({ install: async () => {} }),
      layoutReplayViewport: async (_canvas, size) => { layoutSizes.push(size); },
      replayViewport: async (_canvas, size, { deviceScaleFactor }) => ({
        bitmap: { width: size.width * deviceScaleFactor, height: size.height * deviceScaleFactor },
        bounds: { x: 0, y: 0, ...size }, deviceScaleFactor,
      }),
      window: { ...browserWindow, __noonInspectionTest: inspection },
      document: browserDocument,
      Event: class Event { constructor() {} },
      WheelEvent: class WheelEvent { constructor() {} },
      console: { log: () => events.push("passed"), error: () => events.push("diagnostic:error") },
    });
  } catch (caught) { error = caught; }
  return { error, writes, events, captures, captureNames: names, contextOptions, launchOptions, layoutSizes };
}

function assertRetained(result, names) {
  for (const name of names) {
    const index = result.captureNames.indexOf(name);
    assert.equal(result.writes.get(`${name}.png`), result.captures[index]);
  }
  assert.equal(result.events.filter((event) => event === "browser:close").length, 1);
  assert.equal(result.events.filter((event) => event === "server:kill").length, 1);
  // Artifact I/O must not insert a wait before any original screenshot.
  assert.ok(result.events.findIndex((event) => event.startsWith("write:")) >
    result.events.findLastIndex((event) => event.startsWith("capture:")));
  return JSON.parse(result.writes.get("result.json"));
}

test("successful legacy clear and source-declared indication retain their captures and measurements", async () => {
  const result = await runSmoke();
  assert.equal(result.error, undefined);
  const report = assertRetained(result, ["baseline", "selected", "cleared", "authoredBaseline", "indicated", "restored", "repeated",
    "zoomed", "zoomIndicated", "zoomRestored", "zoomReset"]);
  assert.equal(report.selectedChanged, 501);
  assert.equal(report.clearDifference, 0);
  assert.equal(report.indicatedChanged, 501);
  assert.equal(report.authoredInteraction, "click-indicate");
  assert.equal(report.noJspi, true);
  assert.equal(report.wheelInput, "browser mouse wheel");
  assert.equal(report.error, null);
  assert.equal(result.events.at(-1), "passed");
});

test("default routing preserves desktop Chromium mouse input and 960 by 540 canvas", async () => {
  const result = await runSmoke();
  assert.equal(result.error, undefined);
  assert.deepEqual(result.launchOptions, { browser: "chromium", headless: true });
  assert.deepEqual(JSON.parse(JSON.stringify(result.contextOptions)), { viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  assert.ok(result.events.some((event) => event.startsWith("mouse:")));
  assert.ok(!result.events.some((event) => event.startsWith("touch:")));
  const report = JSON.parse(result.writes.get("result.json"));
  assert.deepEqual(report.captureViewport.bitmap, { width: 960, height: 540 });
  assert.equal(report.captureViewport.deviceScaleFactor, 1);
});

test("mobile profile uses DPR2 portrait geometry, touch input, and a viewport-fitting canvas", async () => {
  const result = await runSmoke({ env: { NOON_PLAYGROUND_BROWSER: "webkit", NOON_PLAYGROUND_PROFILE: "mobile-dpr2" } });
  assert.equal(result.error, undefined);
  assert.deepEqual(result.launchOptions, { browser: "webkit", headless: true });
  assert.deepEqual(JSON.parse(JSON.stringify(result.contextOptions)), {
    viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true,
  });
  assert.ok(result.events.some((event) => event.startsWith("touch:")));
  assert.ok(!result.events.some((event) => event.startsWith("mouse:")));
  const report = JSON.parse(result.writes.get("result.json"));
  assert.equal(report.wheelInput, "DOM wheel (mobile WebKit automation limitation)");
  assert.equal(result.events.filter(event => event === "DOM:wheel").length, 2);
  assert.deepEqual(report.captureViewport.bitmap, { width: 716, height: 402 });
  assert.equal(report.captureViewport.deviceScaleFactor, 2);
  assert.equal(result.layoutSizes.length, 3, "legacy, authored, and native-drag canvases must use the mobile capture layout");
  assert.deepEqual(report.browserErrors, []);
  assert.equal(report.nativeInputDrag.capability, "nonreplayable-native-input");
  assert.equal(report.nativeInputDrag.backend, "WebGL2");
  assert.equal(report.nativeInputDrag.deviceScaleFactor, 2);
  assert.ok(report.nativeInputDrag.pointerSamples.some(sample => sample.type === "pointercancel" && sample.pointerType === "touch"));
  assert.ok(report.nativeInputDrag.releasedDragChangedPixels > 500);
  assert.ok(report.nativeInputDrag.cancelledDragTransientPixels > 500);
  assert.equal(report.nativeInputDrag.cancelledDragRestoresReleasedPixels, true);
  assert.equal(report.nativeInputDrag.runRestoresExactBaseline, true);
  assert.ok(result.events.includes("public:run"));
  assertRetained(result, ["nativeDragBaseline", "nativeDragReleased", "nativeDragCancelTransient", "nativeDragCancelled", "nativeDragRunReset"]);
  for (const size of result.layoutSizes) {
    assert.ok(size.width <= 390 && size.height <= 844, "mobile capture canvas must fit the portrait viewport");
  }
});

test("native drag capture failure retains earlier gesture evidence and records failure", async () => {
  const result = await runSmoke({
    captureFailure: "nativeDragCancelTransient",
    env: { NOON_PLAYGROUND_BROWSER: "webkit", NOON_PLAYGROUND_PROFILE: "mobile-dpr2" },
  });
  assert.match(result.error?.message, /capture failed: nativeDragCancelTransient/);
  const report = assertRetained(result, ["nativeDragBaseline", "nativeDragReleased"]);
  assert.match(report.error, /capture failed: nativeDragCancelTransient/);
  assert.equal(result.writes.has("nativeDragCancelTransient.png"), false);
  assert.ok(!result.events.includes("passed"));
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
  assert.equal(result.events.filter((event) => event.startsWith("mouse:") || event.startsWith("touch:")).length, 1);
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

test("async browser observations poll resolved false values and propagate failures", async () => {
  let reads = 0, waits = 0;
  const page = { evaluate: async (fn, argument) => fn(argument), waitForTimeout: async () => { waits++; } };
  await waitForBrowserObservation(page, async limit => ++reads >= limit, 3);
  assert.equal(reads, 3);
  assert.equal(waits, 2);
  await assert.rejects(waitForBrowserObservation(page, async () => false, null, { timeout: 0 }), /Timed out/);
  await assert.rejects(waitForBrowserObservation(page, async () => { throw new Error("worker failed"); }), /worker failed/);
});

test("gallery qualification observes rejection before offering a distinct fresh wheel", async () => {
  const result = await runSmoke({ wheelReplies: [null, true, true] });
  assert.equal(result.error, undefined);
  const report = JSON.parse(result.writes.get("result.json"));
  assert.deepEqual(report.wheelAcknowledgements, [null, true, true]);
});

test("gallery qualification bounds fresh wheel attempts and never claims rejected zoom", async () => {
  const result = await runSmoke({ wheelReplies: Array(8).fill(null) });
  assert.match(result.error?.message, /did not admit a fresh wheel/);
  const report = JSON.parse(result.writes.get("result.json"));
  assert.deepEqual(report.wheelAcknowledgements, Array(8).fill(null));
  assert.equal(result.writes.has("zoomed.png"), false);
});
