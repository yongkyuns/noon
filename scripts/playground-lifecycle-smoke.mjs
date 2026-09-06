import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

import playwright from "playwright";

const { chromium } = playwright;
const port = Number(process.env.NOON_PLAYGROUND_LIFECYCLE_PORT ?? "4184");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  process.env.NOON_PLAYGROUND_LIFECYCLE_ARTIFACTS ??
    "browser-smoke-artifacts/playground-lifecycle",
);

await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", "."],
  { stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => {
  serverOutput += chunk;
});
server.stderr.on("data", (chunk) => {
  serverOutput += chunk;
});

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Playground server did not start: ${lastError}\n${serverOutput}`);
}

async function snapshot(page) {
  return page.evaluate(() => {
    const status = document.querySelector("#status");
    const patchStatus = document.querySelector("#patch-status");
    return {
      visibilityState: document.visibilityState,
      rendererBackend: status?.dataset.rendererBackend ?? "",
      runtimeState: status?.dataset.state ?? "",
      runtimeStartup: status?.dataset.runtimeStartup ?? "",
      presentedFrames: Number(status?.dataset.presentedFrames ?? "0"),
      executionMode: status?.dataset.executionMode ?? "",
      objectCount: document.querySelector("#metric-objects")?.value ?? "",
      metricTime: document.querySelector("#metric-time")?.value ?? "",
      patchState: patchStatus?.dataset.state ?? "",
      patchText: patchStatus?.value ?? patchStatus?.textContent ?? "",
      canvasIdentity: document.querySelector("#scene")?.dataset.lifecycleSmokeIdentity ?? null,
      canvasCount: document.querySelectorAll("canvas").length,
    };
  });
}

async function startDeferredRuntime(page) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  const deferred = await snapshot(page);
  assert.equal(deferred.runtimeStartup, "deferred", "page load must leave lifecycle runtime deferred");
  assert.equal(deferred.rendererBackend, "", "deferred page load must not initialize a renderer");
  assert.equal(deferred.presentedFrames, 0, "deferred page load must not render frames");
  const visibleExampleCount = await page.evaluate(
    () => window.__noonExampleGallery?.visibleExampleCount ?? Infinity,
  );
  assert.ok(
    Number.isSafeInteger(visibleExampleCount) &&
      visibleExampleCount > 0 &&
      visibleExampleCount <= 18,
    `initial gallery page materialized ${visibleExampleCount} examples`,
  );
  await page.locator("#replace-scene").click();
  await page.waitForFunction(
    () => {
      const status = document.querySelector("#status");
      const patch = document.querySelector("#patch-status");
      return (
        status?.dataset.rendererBackend === "WebGL2" &&
        patch?.dataset.state === "applied" &&
        Number(status?.dataset.presentedFrames ?? "0") > 0
      );
    },
    null,
    { timeout: 60_000 },
  );
}

async function waitForFrameAfter(page, previousFrames, expectedObjectCount, viewport, label) {
  // A completed source-owned scene is intentionally idle. Exercise the public
  // resize path after activation to prove the existing runtime and surface can
  // still invalidate and present work after the page resumes.
  await page.setViewportSize(viewport);
  await page.waitForFunction(
    (previous) => {
      const status = document.querySelector("#status");
      return (
        status?.dataset.state !== "error" &&
        Number(status?.dataset.presentedFrames ?? "0") > previous
      );
    },
    previousFrames,
    { timeout: 10_000 },
  );
  const current = await snapshot(page);
  assert.ok(current.presentedFrames > previousFrames, `${label}: no new frame was presented`);
  assert.equal(current.rendererBackend, "WebGL2", `${label}: renderer backend changed`);
  assert.notEqual(current.runtimeState, "error", `${label}: runtime entered an error state`);
  assert.equal(current.objectCount, expectedObjectCount, `${label}: scene object count changed`);
  assert.equal(current.canvasIdentity, "original", `${label}: runtime replaced the canvas`);
  assert.equal(current.canvasCount, 1, `${label}: runtime created an extra canvas`);
  return current;
}

const diagnostics = {
  browser: null,
  viewport: { width: 1000, height: 700 },
  devicePixelRatio: 1,
  snapshots: {},
  pageErrors: [],
  consoleErrors: [],
  serverOutput: "",
};

let browser = null;
let context = null;
let page = null;
let cdp = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });
  context = await browser.newContext({
    viewport: diagnostics.viewport,
    deviceScaleFactor: diagnostics.devicePixelRatio,
  });
  page = await context.newPage();
  page.on("pageerror", (error) => diagnostics.pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") diagnostics.consoleErrors.push(message.text());
  });

  diagnostics.browser = await browser.version();
  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });
  await startDeferredRuntime(page);
  await page.evaluate(() => {
    document.querySelector("#scene").dataset.lifecycleSmokeIdentity = "original";
  });

  diagnostics.snapshots.baseline = await snapshot(page);
  assert.ok(
    Number(diagnostics.snapshots.baseline.objectCount) > 0,
    "lifecycle baseline must retain authored scene objects",
  );
  assert.equal(diagnostics.snapshots.baseline.canvasCount, 1, "lifecycle baseline must own one canvas");
  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "baseline.png") });

  cdp = await context.newCDPSession(page);
  await cdp.send("Page.setWebLifecycleState", { state: "frozen" });
  await page.waitForTimeout(750);
  diagnostics.snapshots.frozen = await snapshot(page);
  assert.notEqual(
    diagnostics.snapshots.frozen.runtimeState,
    "error",
    "freezing the page must not put the runtime into an error state",
  );
  assert.equal(
    diagnostics.snapshots.frozen.rendererBackend,
    diagnostics.snapshots.baseline.rendererBackend,
    "freezing the page must not replace the renderer backend",
  );

  await cdp.send("Page.setWebLifecycleState", { state: "active" });
  diagnostics.snapshots.resumed = await waitForFrameAfter(
    page,
    diagnostics.snapshots.frozen.presentedFrames,
    diagnostics.snapshots.baseline.objectCount,
    { width: 1001, height: 700 },
    "resume after page freeze",
  );
  assert.equal(
    diagnostics.snapshots.resumed.patchState,
    "applied",
    "resume must retain the active authored scene",
  );

  await cdp.send("Page.setWebLifecycleState", { state: "frozen" });
  await page.waitForTimeout(250);
  await cdp.send("Page.setWebLifecycleState", { state: "active" });
  diagnostics.snapshots.secondResume = await waitForFrameAfter(
    page,
    diagnostics.snapshots.resumed.presentedFrames,
    diagnostics.snapshots.baseline.objectCount,
    diagnostics.viewport,
    "second resume after page freeze",
  );

  assert.equal(
    diagnostics.pageErrors.length,
    0,
    `page lifecycle transitions produced unhandled page errors:\n${diagnostics.pageErrors.join("\n")}`,
  );
  await page.locator("#scene").screenshot({ path: path.join(artifactDir, "resumed.png") });

  diagnostics.serverOutput = serverOutput;
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
  console.log(
    `playground lifecycle recovery ok: ${diagnostics.snapshots.baseline.presentedFrames} -> ` +
      `${diagnostics.snapshots.secondResume.presentedFrames} frames`,
  );
} catch (error) {
  diagnostics.failure = error instanceof Error ? error.stack ?? error.message : String(error);
  diagnostics.serverOutput = serverOutput;
  if (page !== null) {
    try {
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch (screenshotError) {
      diagnostics.screenshotFailure = String(screenshotError);
    }
  }
  await writeFile(
    path.join(artifactDir, "diagnostics.json"),
    `${JSON.stringify(diagnostics, null, 2)}\n`,
  );
  throw error;
} finally {
  await cdp?.detach().catch(() => {});
  await context?.close().catch(() => {});
  await browser?.close().catch(() => {});
  server.kill("SIGTERM");
}
