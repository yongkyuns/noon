import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";
import playwright from "playwright";
import { disableAuthoringJspi, playgroundLaunchOptions, waitForBrowserObservation } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { layoutReplayViewport, replayViewport } from "./showcase-viewport.mjs";
import { assertNonreplayableShowcase, readLiveState } from "./showcase-live-review.mjs";
import { normalizeShowcaseManifest } from "../web/showcase-gallery.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.NOON_GALLERY_SELECTION_PORT ?? 4217);
const base = process.env.NOON_GALLERY_SELECTION_BASE ?? `http://127.0.0.1:${port}/web/`;
const artifacts = path.resolve(
  root,
  process.env.NOON_GALLERY_SELECTION_ARTIFACTS ??
    "browser-smoke-artifacts/gallery-pointer-selection",
);
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? "chromium";
const profileName = process.env.NOON_PLAYGROUND_PROFILE ?? "desktop-dpr1";
const profiles = {
  // Preserve the original gallery-selection smoke's viewport exactly.
  "desktop-dpr1": { viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 },
  "desktop-dpr2": { viewport: { width: 1100, height: 760 }, deviceScaleFactor: 2 },
  "mobile-dpr2": { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true },
};
assert.ok(["chromium", "firefox", "webkit"].includes(browserName), `unknown playground browser: ${browserName}`);
assert.ok(profileName in profiles, `unknown playground profile: ${profileName}`);
const profile = profiles[profileName];
const browserType = playwright[browserName];
const captureSize = profileName.startsWith("mobile")
  ? { width: profile.viewport.width - 32, height: Math.floor((profile.viewport.width - 32) * 9 / 16) }
  : { width: 960, height: 540 };
const tap = (page, x, y) => profile.hasTouch ? page.touchscreen.tap(x, y) : page.mouse.click(x, y);
await mkdir(artifacts, { recursive: true });

let server;
let browser;
let runtimeCache;
const captures = {};
const report = {
  browser: browserName, profile: profileName, input: profile.hasTouch ? "touch" : "mouse",
  legacyInteraction: "pointer-fill-selection", authoredInteraction: "click-indicate", noJspi: true,
};
let failure;

function changedPixels(leftBytes, rightBytes) {
  const left = PNG.sync.read(leftBytes);
  const right = PNG.sync.read(rightBytes);
  assert.equal(left.width, right.width);
  assert.equal(left.height, right.height);
  let changed = 0;
  for (let i = 0; i < left.data.length; i += 4) {
    if (
      left.data[i] !== right.data[i] ||
      left.data[i + 1] !== right.data[i + 1] ||
      left.data[i + 2] !== right.data[i + 2] ||
      left.data[i + 3] !== right.data[i + 3]
    ) {
      changed += 1;
    }
  }
  return changed;
}

async function waitForPresentation(page, previous) {
  await page.waitForFunction(
    () =>
      window.__noonExampleGallery !== undefined &&
      document.querySelector("#patch-status")?.dataset.state !== "error",
  );
  await waitForBrowserObservation(page,
    async (prior) => {
      const metrics = await window.__noonExampleGallery.executionMetrics();
      return Number(metrics?.metrics?.presentedFrames ?? 0) > prior;
    },
    previous,
    { timeout: 15000 },
  );
}

async function waitForAuthoring(page) {
  await page.waitForFunction(() => {
    const state = document.querySelector("#patch-status")?.dataset.state;
    return state === "error" || state === "applied" && !window.__noonExampleGallery.runInFlight;
  }, null, { timeout: 60000 });
  assert.equal(await page.locator("#patch-status").getAttribute("data-state"), "applied",
    await page.evaluate(() => document.querySelector("#patch-status")?.value));
}

async function waitForExactPixels(canvas, baseline, label) {
  let last;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    last = await canvas.screenshot();
    if (changedPixels(baseline, last) === 0) return last;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const error = new Error(`${label} did not restore the exact baseline pixels`);
  error.capture = last;
  throw error;
}

async function presentedFrames(page) {
  return page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
}

async function assertSettled(page, label) {
  const before = await presentedFrames(page);
  await page.waitForTimeout(250);
  assert.equal(await presentedFrames(page), before, `${label} left a frame wake active`);
}

try {
  if (!process.env.NOON_GALLERY_SELECTION_BASE) {
    server = spawn(
      "python3",
      ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", root],
      { stdio: "ignore" },
    );
    let ready = false;
    for (let i = 0; i < 100; i += 1) {
      ready = await fetch(base).then((response) => response.ok).catch(() => false);
      if (ready) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    assert.ok(ready, "gallery selection HTTP server did not start");
  }

  const worker = await fetch(new URL("python-worker.js", base));
  assert.ok(worker.ok, "python worker source is unavailable");
  runtimeCache = createPyodideResourceCache(await worker.text());

  browser = await browserType.launch(playgroundLaunchOptions(browserName));
  const context = await browser.newContext(profile);
  await runtimeCache.install(context);
  await disableAuthoringJspi(context);
  const page = await context.newPage();
  page.setDefaultTimeout(30000);
  const errors = [];
  report.browserErrors = errors;
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().startsWith("Failed to load resource:")) {
      // Curated posters may be intentionally pending while the browser-runtime
      // gate runs. Keep these visible in the report without treating them as
      // Rust/input failures.
      report.resourceWarnings ??= [];
      report.resourceWarnings.push(message.text());
    } else if (message.type() === "error" || message.type() === "warning" &&
        /Recoverable Python callback error|\[Noon input\]/.test(message.text())) errors.push(message.text());
  });

  await page.goto(`${base}?example=noon-pointer-selection`, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  assert.equal(
    await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
    "noon-pointer-selection",
  );
  const canvas = page.locator("#scene");
  // Compare the renderer's pixels, not rounded-corner browser antialiasing.
  // Keep real host pointer events enabled on the existing canvas.
  await layoutReplayViewport(canvas, captureSize);
  await canvas.evaluate(element => element.style.setProperty("pointer-events", "auto", "important"));
  await page.evaluate(() => window.__noonExampleGallery.run());
  await waitForAuthoring(page);
  assert.equal(
    await page.evaluate(() => document.querySelector("#status")?.dataset.interaction),
    "pointer-fill-selection",
    "gallery manifest must enable selection on the public runtime",
  );

  report.captureViewport = await replayViewport(canvas, captureSize, { deviceScaleFactor: profile.deviceScaleFactor });
  const box = await canvas.boundingBox();
  assert.ok(box && box.width > 0 && box.height > 0, "gallery canvas is not drawable");
  const baseline = await canvas.screenshot();
  captures.baseline = baseline;

  const beforeSelect = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await tap(page,
    box.x + box.width / 2 - box.height / 4,
    box.y + box.height / 2,
  );
  await waitForPresentation(page, beforeSelect);
  const selected = await canvas.screenshot();
  captures.selected = selected;
  const selectedChanged = changedPixels(baseline, selected);
  report.selectedChanged = selectedChanged;
  assert.ok(selectedChanged > 500, `selection changed only ${selectedChanged} pixels`);

  const beforeClear = await page.evaluate(async () =>
    Number((await window.__noonExampleGallery.executionMetrics()).metrics.presentedFrames),
  );
  await tap(page, box.x + 18, box.y + 18);
  await waitForPresentation(page, beforeClear);
  const cleared = await canvas.screenshot();
  captures.cleared = cleared;
  const clearDifference = changedPixels(baseline, cleared);
  report.clearDifference = clearDifference;
  assert.equal(clearDifference, 0, "background clear must restore the authored image exactly");
  assert.deepEqual(errors, []);

  report.legacyRenderer = await page.evaluate(() => document.querySelector("#status")?.dataset.rendererBackend);

  // The curated lesson carries no manifest interaction policy. Its Rust-owned
  // click declaration must still work after the authored introduction has
  // finished and replay has been paused at that endpoint.
  await page.goto(`${base}?catalog=showcase&example=showcase-pointer-selection`, { waitUntil: "domcontentloaded" });
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  assert.equal(
    await page.evaluate(() => window.__noonExampleGallery.selectedExampleId),
    "showcase-pointer-selection",
  );
  const source = await fetch(new URL("python/examples/showcase_pointer_selection.py", base)).then((response) => response.text());
  assert.match(source, /\.on_click\s*\(/, "showcase source must declare its click action");
  await page.evaluate(async () => {
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const original = ExecutionWorkerClient.prototype.scrollInspectionView;
    window.__noonInspectionTest = { pending: [], samples: [] };
    ExecutionWorkerClient.prototype.scrollInspectionView = function(...args) {
      window.__noonInspectionTest.samples.push({ input: { ...args[0] }, receipt: this.pointerPresentation });
      const pending = original.apply(this, args);
      window.__noonInspectionTest.pending.push(pending);
      return pending;
    };
  });
  const authoredCanvas = page.locator("#scene");
  await layoutReplayViewport(authoredCanvas, captureSize);
  await authoredCanvas.evaluate(element => element.style.setProperty("pointer-events", "auto", "important"));
  await page.evaluate(() => window.__noonExampleGallery.run());
  await waitForAuthoring(page);
  assert.equal(
    await page.evaluate(() => document.querySelector("#status")?.dataset.interaction),
    "none",
    "source-declared click actions must not require manifest interaction policy",
  );
  const pause = page.getByRole("button", { name: "Pause animation", exact: true });
  if (await pause.count()) await pause.click();
  const scrubber = page.locator(".playback-scrubber");
  const endpoint = Number(await scrubber.getAttribute("max"));
  await scrubber.evaluate((input, time) => {
    input.value = String(time);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }, endpoint);
  await page.waitForFunction((time) => {
    const controls = document.querySelector(".playback-controls");
    return controls?.dataset.busy === "false" &&
      controls.querySelector(".playback-toggle")?.getAttribute("aria-label") === "Play animation" &&
      Math.abs(Number(controls.dataset.elapsedSeconds) - time) < 1e-7;
  }, endpoint);
  const authoredBox = await authoredCanvas.boundingBox();
  assert.ok(authoredBox && authoredBox.width > 0 && authoredBox.height > 0, "authored canvas is not drawable");
  const authoredBaseline = await authoredCanvas.screenshot();
  captures.authoredBaseline = authoredBaseline;
  const clickCircle = () => tap(page,
    authoredBox.x + authoredBox.width * 0.36,
    authoredBox.y + authoredBox.height * 0.5,
  );
  const waitChanged = async (baseline, label) => {
    for (let attempt = 0; attempt < 40; attempt += 1) {
      const image = await authoredCanvas.screenshot();
      if (changedPixels(baseline, image) > 500) return image;
      await page.waitForTimeout(25);
    }
    throw new Error(`${label} did not change trusted canvas pixels`);
  };
  await clickCircle();
  const indicated = await waitChanged(authoredBaseline, "first authored click");
  captures.indicated = indicated;
  report.indicatedChanged = changedPixels(authoredBaseline, indicated);
  const restored = await waitForExactPixels(authoredCanvas, authoredBaseline, "first authored click");
  captures.restored = restored;
  await assertSettled(page, "first authored click");
  await clickCircle();
  const repeated = await waitChanged(authoredBaseline, "repeated authored click");
  captures.repeated = repeated;
  await waitForExactPixels(authoredCanvas, authoredBaseline, "repeated authored click");
  await assertSettled(page, "repeated authored click");
  const beforeBackground = await presentedFrames(page);
  await tap(page, authoredBox.x + 12, authoredBox.y + 12);
  await page.waitForTimeout(150);
  assert.equal(changedPixels(authoredBaseline, await authoredCanvas.screenshot()), 0, "background click must not change the source-declared scene");
  assert.equal(await presentedFrames(page), beforeBackground, "background click must not create interaction work");
  // Exercise the actual gallery opt-in, not a manually constructed client.
  const domWheel = browserName === "webkit" && profile.hasTouch;
  report.wheelInput = domWheel ? "DOM wheel (mobile WebKit automation limitation)" : "browser mouse wheel";
  const dispatchWheel = async delta => {
    if (domWheel) {
      // Playwright cannot drive a wheel in mobile WebKit. Keep the same DOM
      // collector and real touch picking, without claiming native wheel input.
      await authoredCanvas.evaluate((canvas, deltaY) => {
        const rect = canvas.getBoundingClientRect();
        const event = new WheelEvent("wheel", { cancelable: true, deltaMode: 0, deltaY,
          clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 });
        canvas.dispatchEvent(event);
        if (!event.defaultPrevented) throw new Error("gallery inspection did not consume DOM wheel");
      }, delta);
    } else {
      await page.mouse.move(authoredBox.x + authoredBox.width / 2, authoredBox.y + authoredBox.height / 2);
      await page.mouse.wheel(0, delta);
    }
  };
  report.wheelAcknowledgements = [];
  const wheel = async delta => {
    for (let attempt = 0; attempt < 8; attempt++) {
      const count = await page.evaluate(() => window.__noonInspectionTest.pending.length);
      await dispatchWheel(delta);
      const accepted = await page.evaluate(async count => {
        const pending = window.__noonInspectionTest.pending;
        return pending.length === count ? null : (await pending.at(-1)).inspectionScrollChanged;
      }, count);
      report.wheelAcknowledgements.push(accepted);
      report.wheelSamples = await page.evaluate(() => window.__noonInspectionTest.samples);
      if (accepted === true) return;
      assert.equal(accepted, null, "gallery zoom unexpectedly admitted a no-op");
      // A new occurrence uses a new collection-time receipt; no rejected input
      // is queued or relabelled as current by either the test or production host.
      await page.waitForTimeout(25);
    }
    throw new Error("gallery inspection did not admit a fresh wheel occurrence");
  };
  const beforeZoom = await presentedFrames(page);
  await wheel(-500 * Math.log(2));
  await waitForPresentation(page, beforeZoom);
  const zoomed = await authoredCanvas.screenshot();
  captures.zoomed = zoomed;
  report.zoomChanged = changedPixels(authoredBaseline, zoomed);
  assert.ok(report.zoomChanged > 500, "gallery wheel must change the composed view");
  await tap(page, authoredBox.x + authoredBox.width * 0.22, authoredBox.y + authoredBox.height * 0.5);
  captures.zoomIndicated = await waitChanged(zoomed, "click through zoomed gallery view");
  captures.zoomRestored = await waitForExactPixels(authoredCanvas, zoomed, "zoomed authored click");
  await assertSettled(page, "zoomed authored click");
  const beforeZoomOut = await presentedFrames(page);
  await wheel(500 * Math.log(2));
  await waitForPresentation(page, beforeZoomOut);
  captures.zoomReset = await waitForExactPixels(authoredCanvas, authoredBaseline, "inverse gallery zoom");
  await assertSettled(page, "inverse gallery zoom");

  if (browserName === "webkit" && profileName === "mobile-dpr2") {
    const rawManifest = await fetch(new URL("python/examples/noon_showcase_manifest.json", base)).then(response => response.json());
    const normalizedEntry = normalizeShowcaseManifest(rawManifest).examples.find(entry => entry.id === "showcase-translation-drag");
    const declaredEntry = rawManifest.entries.find(entry => entry.id === "showcase-translation-drag");
    const nativeInputEntry = normalizedEntry && declaredEntry
      ? { ...normalizedEntry, duration: declaredEntry.duration }
      : null;
    assert.ok(nativeInputEntry, "native-input drag lesson is missing from the curated showcase");
    assert.equal(nativeInputEntry.playbackCapability, "nonreplayable-native-input");
    await page.goto(`${base}?catalog=showcase&example=${nativeInputEntry.id}`, { waitUntil: "domcontentloaded" });
    await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
    assert.equal(await page.evaluate(() => window.__noonExampleGallery.selectedExampleId), nativeInputEntry.id);
    const dragCanvas = page.locator("#scene");
    await layoutReplayViewport(dragCanvas, captureSize);
    await dragCanvas.evaluate(element => element.style.setProperty("pointer-events", "auto", "important"));
    await page.evaluate(() => window.__noonExampleGallery.run());
    await waitForAuthoring(page);
    await page.waitForFunction(duration => {
      const controls = document.querySelector(".playback-controls")?.dataset;
      return Number(controls?.elapsedSeconds) >= duration - 1e-7;
    }, nativeInputEntry.duration, { timeout: 30000 });
    report.nativeDragViewport = await replayViewport(dragCanvas, captureSize, { deviceScaleFactor: profile.deviceScaleFactor });
    const nativeBackend = await page.locator("#status").getAttribute("data-renderer-backend");
    assert.ok(["WebGL2", "WebGPU"].includes(nativeBackend), "native drag did not use a real renderer backend");
    const firstPassState = await readLiveState(page);
    assertNonreplayableShowcase(nativeInputEntry, firstPassState, nativeBackend);
    const dragBox = await dragCanvas.boundingBox();
    assert.ok(dragBox && dragBox.width > 0 && dragBox.height > 0, "native drag canvas is not drawable");
    const nativeBaseline = await dragCanvas.screenshot();
    captures.nativeDragBaseline = nativeBaseline;
    const assertNoNativeInputError = async stage => {
      const state = await page.evaluate(() => ({
        patchState: document.querySelector("#patch-status")?.dataset.state,
        patchText: document.querySelector("#patch-status")?.value,
        runtimeText: document.querySelector("#status-text")?.textContent,
      }));
      assert.notEqual(state.patchState, "error", `${stage}: native/Rust input failed: ${state.patchText || state.runtimeText}`);
      assert.doesNotMatch(state.runtimeText || "", /^Error:/, `${stage}: runtime reported an error`);
      assert.deepEqual(errors, [], `${stage}: browser reported a Rust/input error`);
    };
    const touchPoint = (x, y) => ({ clientX: dragBox.x + dragBox.width * x, clientY: dragBox.y + dragBox.height * y });
    const dispatchTouchPointer = async (type, point, buttons, button = -1) => dragCanvas.evaluate((canvas, packet) => {
      const event = new PointerEvent(packet.type, {
        bubbles: true, cancelable: true, pointerId: 73, pointerType: "touch", isPrimary: true,
        clientX: packet.clientX, clientY: packet.clientY, button: packet.button, buttons: packet.buttons,
      });
      return canvas.dispatchEvent(event);
    }, { type, ...point, buttons, button });
    const waitForNativeChange = async (baseline, label) => {
      for (let attempt = 0; attempt < 60; attempt += 1) {
        const image = await dragCanvas.screenshot();
        if (changedPixels(baseline, image) > 500) return image;
        await page.waitForTimeout(25);
      }
      throw new Error(`${label} did not produce visible Rust-owned translation`);
    };
    const samples = [];
    const sendSample = async (type, x, y, buttons, button = -1) => {
      const point = touchPoint(x, y);
      samples.push({ type, pointerType: "touch", x, y, buttons });
      await dispatchTouchPointer(type, point, buttons, button);
    };
    const preReleaseFrames = await presentedFrames(page);
    await sendSample("pointerdown", 0.68, 0.5, 1, 0);
    await sendSample("pointermove", 0.72, 0.5, 1);
    await sendSample("pointermove", 0.77, 0.5, 1);
    await sendSample("pointerup", 0.82, 0.5, 0, 0);
    await assertNoNativeInputError("touch release");
    await waitForPresentation(page, preReleaseFrames);
    const released = await waitForNativeChange(nativeBaseline, "released touch drag");
    captures.nativeDragReleased = released;
    await sendSample("pointermove", 0.9, 0.6, 0);
    await assertSettled(page, "released touch hover");
    assert.equal(changedPixels(released, await dragCanvas.screenshot()), 0, "released touch continued translating the rectangle");

    const beforeCancelFrames = await presentedFrames(page);
    await sendSample("pointerdown", 0.82, 0.5, 1, 0);
    await sendSample("pointermove", 0.76, 0.5, 1);
    await waitForPresentation(page, beforeCancelFrames);
    const cancelTransient = await waitForNativeChange(released, "second touch drag before cancellation");
    captures.nativeDragCancelTransient = cancelTransient;
    await sendSample("pointercancel", 0.76, 0.5, 0);
    await assertNoNativeInputError("touch cancellation");
    const cancelled = await waitForExactPixels(dragCanvas, released, "touch cancellation rollback");
    captures.nativeDragCancelled = cancelled;
    await assertSettled(page, "cancelled native drag");
    await sendSample("pointermove", 0.58, 0.5, 1);
    await assertSettled(page, "cancelled touch continuation");
    assert.equal(changedPixels(cancelled, await dragCanvas.screenshot()), 0, "cancelled touch continued translating the rectangle");

    await dragCanvas.evaluate(element => element.style.setProperty("pointer-events", "none", "important"));
    await page.locator("#replace-scene").click();
    await waitForAuthoring(page);
    await page.waitForFunction(duration => {
      const controls = document.querySelector(".playback-controls")?.dataset;
      return Number(controls?.elapsedSeconds) >= duration - 1e-7;
    }, nativeInputEntry.duration, { timeout: 30000 });
    assertNonreplayableShowcase(nativeInputEntry, await readLiveState(page), nativeBackend);
    const nativeReset = await waitForExactPixels(dragCanvas, nativeBaseline, "public Run after touch drag/cancel");
    captures.nativeDragRunReset = nativeReset;
    report.nativeInputDrag = {
      capability: nativeInputEntry.playbackCapability,
      limitation: nativeInputEntry.playbackLimitation,
      backend: nativeBackend,
      deviceScaleFactor: profile.deviceScaleFactor,
      pointerSamples: samples,
      releasedDragChangedPixels: changedPixels(nativeBaseline, released),
      cancelledDragTransientPixels: changedPixels(released, cancelTransient),
      cancelledDragRestoresReleasedPixels: changedPixels(released, cancelled) === 0,
      runRestoresExactBaseline: true,
    };
    assert.deepEqual(errors, [], "Rust/browser input path reported an error during native drag");
    assert.equal(await page.locator("#patch-status").getAttribute("data-state"), "applied",
      "Rust input failure changed the public Run state");
  }

  assert.equal(await page.evaluate(() => window.__noonNoJspiWorkerWrapped), true,
    "selection smoke must run the production authoring worker without JSPI");
  report.authoredRenderer = await page.evaluate(() => document.querySelector("#status")?.dataset.rendererBackend);
  assert.deepEqual(errors, []);
} catch (error) {
  failure = error;
  if (error.capture) captures.failure = error.capture;
  throw error;
} finally {
  // Persist the original captures after the verdict, including failed/partial
  // runs. Do not insert artifact I/O, extra captures or waits into the sample path.
  try {
    const writes = await Promise.allSettled([
      ...Object.entries(captures).map(([name, bytes]) =>
        writeFile(path.join(artifacts, `${name}.png`), bytes),
      ),
      writeFile(
        path.join(artifacts, "result.json"),
        `${JSON.stringify({ ...report, error: failure ? String(failure) : null }, null, 2)}\n`,
      ),
    ]);
    const rejected = writes.find((write) => write.status === "rejected");
    if (rejected) {
      if (!failure) throw rejected.reason;
      // Failure to retain diagnostics must not hide the original assertion.
      console.error("Could not retain gallery selection diagnostics:", rejected.reason);
    }
  } finally {
    try {
      await browser?.close();
    } finally {
      server?.kill("SIGTERM");
    }
  }
}
console.log(
  `Gallery pointer interactions passed: legacy=${report.selectedChanged}, authored=${report.indicatedChanged}`,
);
