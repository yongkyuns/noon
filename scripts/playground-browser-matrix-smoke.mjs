import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? "chromium";
const profileName = process.env.NOON_PLAYGROUND_PROFILE ?? "desktop-dpr1";
const port = Number(process.env.NOON_PLAYGROUND_MATRIX_PORT ?? "4175");
const baseUrl = `http://127.0.0.1:${port}`;
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    `browser-smoke-artifacts/playground-matrix/${browserName}-${profileName}`,
);

const profiles = {
  "desktop-dpr1": { viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 },
  "desktop-dpr2": { viewport: { width: 1100, height: 760 }, deviceScaleFactor: 2 },
  "mobile-dpr2": { viewport: { width: 390, height: 844 }, deviceScaleFactor: 2 },
};

assert.ok(
  ["chromium", "firefox", "webkit"].includes(browserName),
  `unknown playground browser: ${browserName}`,
);
assert.ok(profileName in profiles, `unknown playground profile: ${profileName}`);

const browserType = playwright[browserName];
const profile = profiles[profileName];
await mkdir(artifactDir, { recursive: true });

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
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
  throw new Error(`Playground matrix server did not start: ${lastError}\n${serverOutput}`);
}

function launchOptions() {
  if (browserName === "chromium") {
    return {
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
    };
  }
  if (browserName === "firefox") {
    return {
      headless: true,
      firefoxUserPrefs: {
        "webgl.disabled": false,
        "webgl.force-enabled": true,
      },
    };
  }
  return { headless: true };
}

async function capabilityProbe(page) {
  return page.evaluate(async () => {
    const probeCanvas = document.createElement("canvas");
    let webgl2 = false;
    let offscreenWebgl2 = false;
    let transferredWorkerWebgl2 = false;
    let transferredWorkerWebgl2Error = "";
    try {
      webgl2 = probeCanvas.getContext("webgl2") !== null;
    } catch {
      webgl2 = false;
    }
    if (typeof OffscreenCanvas === "function") {
      try {
        offscreenWebgl2 = new OffscreenCanvas(2, 2).getContext("webgl2") !== null;
      } catch {
        offscreenWebgl2 = false;
      }
    }

    const canTransferToWorker =
      typeof Worker === "function" &&
      typeof HTMLCanvasElement.prototype.transferControlToOffscreen === "function";
    if (canTransferToWorker) {
      const workerSource = `
        self.onmessage = (event) => {
          try {
            const context = event.data.canvas.getContext("webgl2");
            self.postMessage({ ok: context !== null, error: "" });
          } catch (error) {
            self.postMessage({ ok: false, error: String(error) });
          }
        };
      `;
      const workerUrl = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
      const worker = new Worker(workerUrl);
      try {
        const transferCanvas = document.createElement("canvas");
        transferCanvas.width = 2;
        transferCanvas.height = 2;
        const offscreen = transferCanvas.transferControlToOffscreen();
        const result = await new Promise((resolve) => {
          const timeout = setTimeout(
            () => resolve({ ok: false, error: "worker WebGL2 probe timed out" }),
            5000,
          );
          worker.onmessage = (event) => {
            clearTimeout(timeout);
            resolve(event.data);
          };
          worker.onerror = (event) => {
            clearTimeout(timeout);
            resolve({ ok: false, error: event.message || "worker WebGL2 probe crashed" });
          };
          worker.postMessage({ canvas: offscreen }, [offscreen]);
        });
        transferredWorkerWebgl2 = result?.ok === true;
        transferredWorkerWebgl2Error = typeof result?.error === "string" ? result.error : "";
      } catch (error) {
        transferredWorkerWebgl2 = false;
        transferredWorkerWebgl2Error = String(error);
      } finally {
        worker.terminate();
        URL.revokeObjectURL(workerUrl);
      }
    }

    return {
      webAssembly: typeof WebAssembly === "object",
      worker: typeof Worker === "function",
      offscreenCanvas: typeof OffscreenCanvas === "function",
      transferControlToOffscreen:
        typeof HTMLCanvasElement.prototype.transferControlToOffscreen === "function",
      webgl2,
      offscreenWebgl2,
      transferredWorkerWebgl2,
      transferredWorkerWebgl2Error,
      webgpu: typeof navigator.gpu !== "undefined",
      userAgent: navigator.userAgent,
      devicePixelRatio: window.devicePixelRatio,
      viewport: { width: window.innerWidth, height: window.innerHeight },
    };
  });
}

function missingRuntimeCapabilities(capabilities) {
  return [
    ["WebAssembly", capabilities.webAssembly],
    ["Worker", capabilities.worker],
    ["OffscreenCanvas", capabilities.offscreenCanvas],
    ["transferControlToOffscreen", capabilities.transferControlToOffscreen],
    ["WebGL2", capabilities.webgl2],
    ["OffscreenCanvas WebGL2", capabilities.offscreenWebgl2],
    ["transferred worker WebGL2", capabilities.transferredWorkerWebgl2],
  ]
    .filter(([, available]) => !available)
    .map(([name]) => name);
}

async function shellSnapshot(page) {
  return page.evaluate(() => {
    const canvas = document.querySelector("#scene");
    const workspace = document.querySelector(".workspace");
    const run = document.querySelector("#replace-scene");
    const editor = document.querySelector("#python-scene-source");
    const canvasRect = canvas?.getBoundingClientRect() ?? null;
    const documentWidth = document.documentElement.scrollWidth;
    return {
      hasCanvas: canvas !== null,
      hasWorkspace: workspace !== null,
      hasRun: run !== null,
      hasEditor: editor !== null,
      canvasRect:
        canvasRect === null
          ? null
          : { width: canvasRect.width, height: canvasRect.height, x: canvasRect.x, y: canvasRect.y },
      documentWidth,
      viewportWidth: window.innerWidth,
    };
  });
}

function assertShell(snapshot, label) {
  assert.equal(snapshot.hasCanvas, true, `${label}: public canvas is missing`);
  assert.equal(snapshot.hasWorkspace, true, `${label}: public workspace is missing`);
  assert.equal(snapshot.hasRun, true, `${label}: Run control is missing`);
  assert.equal(snapshot.hasEditor, true, `${label}: source editor is missing`);
  assert.ok(snapshot.canvasRect?.width > 0, `${label}: canvas must have positive width`);
  assert.ok(snapshot.canvasRect?.height > 0, `${label}: canvas must have positive height`);
  assert.ok(
    snapshot.documentWidth <= snapshot.viewportWidth + 1,
    `${label}: page overflowed horizontally (${snapshot.documentWidth}px > ${snapshot.viewportWidth}px)`,
  );
}

async function runtimeSnapshot(page) {
  return page.evaluate(() => {
    const patch = document.querySelector("#patch-status");
    const status = document.querySelector("#status");
    return {
      rendererBackend: status?.dataset.rendererBackend ?? null,
      executionMode: status?.dataset.executionMode ?? null,
      runtimeStartup: status?.dataset.runtimeStartup ?? null,
      statusState: status?.dataset.state ?? null,
      statusText: document.querySelector("#status-text")?.textContent ?? "",
      patchState: patch?.dataset.state ?? null,
      patchText: patch?.value ?? patch?.textContent ?? "",
      selectedExampleId:
        document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId ?? null,
      visibleExampleCount: window.__noonExampleGallery?.visibleExampleCount ?? null,
      hasPlaybackControls: document.querySelector(".playback-controls") !== null,
      canvases: document.querySelectorAll("canvas").length,
    };
  });
}

async function waitForAppliedScene(page, expectedExampleId, timeout = 60_000) {
  await page.waitForFunction(
    (id) => {
      const patch = document.querySelector("#patch-status");
      const selected = document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId;
      if (patch?.dataset.state === "error") {
        return true;
      }
      return selected === id && patch?.dataset.state === "applied";
    },
    expectedExampleId,
    { timeout },
  );
  const runtime = await runtimeSnapshot(page);
  assert.equal(
    runtime.patchState,
    "applied",
    `${browserName}/${profileName}: ${expectedExampleId} failed: ${runtime.patchText} ${runtime.statusText}`,
  );
  assert.equal(
    runtime.selectedExampleId,
    expectedExampleId,
    `${browserName}/${profileName}: ${expectedExampleId} did not remain selected`,
  );
  return runtime;
}

async function assertDeferredRuntime(page) {
  await page.waitForFunction(() => window.__noonExampleGallery !== undefined);
  const runtime = await runtimeSnapshot(page);
  assert.ok(
    ["deferred", "preparing-on-run", "prepared-on-run"].includes(runtime.runtimeStartup),
    `${browserName}/${profileName}: unexpected pre-Run startup state ${runtime.runtimeStartup}`,
  );
  assert.equal(
    runtime.executionMode,
    null,
    `${browserName}/${profileName}: deferred page already owns an execution mode`,
  );
  assert.equal(
    runtime.hasPlaybackControls,
    false,
    `${browserName}/${profileName}: deferred page allocated playback controls`,
  );
  assert.ok(
    Number(runtime.visibleExampleCount) <= 18,
    `${browserName}/${profileName}: gallery materialized ${runtime.visibleExampleCount} cards`,
  );
  return runtime;
}

async function chooseDifferentExample(page) {
  const trigger = page.locator("#example-browser-trigger");
  await trigger.waitFor({ state: "visible" });
  assert.equal(
    await trigger.getAttribute("aria-expanded"),
    "false",
    `${browserName}/${profileName}: Examples browser should start closed`,
  );
  await trigger.click();

  const browserLayer = page.locator(".example-browser-layer");
  await browserLayer.waitFor({ state: "visible" });
  assert.equal(
    await trigger.getAttribute("aria-expanded"),
    "true",
    `${browserName}/${profileName}: Examples trigger did not enter expanded state`,
  );

  const targetId = await page.evaluate(() => {
    const selected = document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId;
    return [...document.querySelectorAll(".example-card")]
      .map((card) => card.dataset.exampleId)
      .find((id) => id && id !== selected);
  });
  assert.ok(targetId, `${browserName}/${profileName}: gallery must expose a second example`);
  await page.locator(`.example-card[data-example-id="${targetId}"]`).click();
  await browserLayer.waitFor({ state: "hidden" });
  assert.equal(
    await trigger.getAttribute("aria-expanded"),
    "false",
    `${browserName}/${profileName}: Examples browser did not close after selection`,
  );
  await waitForAppliedScene(page, targetId);
  return targetId;
}

async function editAndRerun(page, expectedExampleId) {
  const editMarker = `# cross-browser matrix ${browserName} ${profileName}`;
  await page.evaluate((marker) => {
    const editor = document.querySelector("#python-scene-source");
    if (!(editor instanceof HTMLTextAreaElement)) {
      throw new Error("Python scene textarea is unavailable");
    }
    if (!editor.value.includes(marker)) {
      editor.value = `${editor.value.trimEnd()}\n\n${marker}\n`;
      editor.dispatchEvent(new Event("input", { bubbles: true }));
    }
  }, editMarker);

  const runButton = page.locator("#replace-scene");
  await runButton.waitFor({ state: "attached" });
  assert.equal(await runButton.isDisabled(), false, `${browserName}/${profileName}: Run stayed disabled`);
  await runButton.click();
  await waitForAppliedScene(page, expectedExampleId);

  // CodeMirror installs a JS accessor on the hidden textarea. Playwright's
  // inputValue() reads the native backing value and bypasses that accessor, so
  // inspect the same stable integration surface that the playground itself uses.
  const source = await page.evaluate(() => document.querySelector("#python-scene-source")?.value ?? "");
  assert.ok(source.includes(editMarker), `${browserName}/${profileName}: edited source was not retained`);
}

async function exerciseResize(page) {
  const sequence =
    profileName === "mobile-dpr2"
      ? [
          { width: 430, height: 760 },
          { width: 360, height: 780 },
          { width: 390, height: 844 },
        ]
      : [
          { width: 980, height: 700 },
          { width: 760, height: 720 },
          profile.viewport,
        ];
  for (const viewport of sequence) {
    await page.setViewportSize(viewport);
  }
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  const shell = await shellSnapshot(page);
  assertShell(shell, `${browserName}/${profileName} after resize`);
}

async function writeDiagnostics(fileName, diagnostics) {
  await writeFile(path.join(artifactDir, fileName), `${JSON.stringify(diagnostics, null, 2)}\n`, "utf8");
}

let browser = null;
let page = null;
const pageErrors = [];
const consoleErrors = [];
let capabilities = null;
let runtimeSupported = null;
let finalRuntime = null;

try {
  await waitForServer();
  browser = await browserType.launch(launchOptions());
  const context = await browser.newContext({
    viewport: profile.viewport,
    deviceScaleFactor: profile.deviceScaleFactor,
  });
  page = await context.newPage();
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });

  // Probe the exact transferred-canvas worker path before Noon starts. Main-thread
  // OffscreenCanvas WebGL2 is not sufficient: WebKit can advertise it while
  // returning null only after an HTML canvas is transferred into a worker.
  capabilities = await capabilityProbe(page);
  assert.equal(
    capabilities.devicePixelRatio,
    profile.deviceScaleFactor,
    `${browserName}/${profileName}: unexpected DPR`,
  );
  const missing = missingRuntimeCapabilities(capabilities);
  runtimeSupported = missing.length === 0;

  await page.goto(`${baseUrl}/web/index.html?example=parity-square-and-circle`, {
    waitUntil: "load",
  });

  const initialShell = await shellSnapshot(page);
  assertShell(initialShell, `${browserName}/${profileName}`);
  finalRuntime = await assertDeferredRuntime(page);

  if (!runtimeSupported) {
    await page.screenshot({ path: path.join(artifactDir, "unsupported.png"), fullPage: true });
    await writeDiagnostics("diagnostics.json", {
      browser: browserName,
      browserVersion: browser.version(),
      profile: profileName,
      runtimeSupported: false,
      missingCapabilities: missing,
      capabilities,
      runtime: finalRuntime,
      pageErrors,
      consoleErrors,
    });
    console.log(
      `↷ ${browserName}/${profileName}: runtime unsupported by capability probe (${missing.join(", ")})`,
    );
  } else {
    const runButton = page.locator("#replace-scene");
    // Gallery metadata can appear before the asynchronously selected source loads.
    // Observe readiness before asserting or clicking the Run control.
    await page.waitForFunction(() => !document.querySelector("#replace-scene")?.disabled);
    assert.equal(await runButton.isDisabled(), false, `${browserName}/${profileName}: Run stayed disabled`);
    await runButton.click();
    finalRuntime = await waitForAppliedScene(page, "parity-square-and-circle");
    assert.ok(
      finalRuntime.rendererBackend === "WebGL2" || finalRuntime.rendererBackend === "WebGPU",
      `${browserName}/${profileName}: unexpected renderer backend ${finalRuntime.rendererBackend}`,
    );
    assert.equal(finalRuntime.canvases, 1, `${browserName}/${profileName}: expected one live canvas`);

    const selectedExampleId = await chooseDifferentExample(page);
    await editAndRerun(page, selectedExampleId);
    await exerciseResize(page);
    finalRuntime = await runtimeSnapshot(page);

    assert.deepEqual(
      pageErrors,
      [],
      `${browserName}/${profileName}: page errors:\n${pageErrors.join("\n")}`,
    );
    assert.deepEqual(
      consoleErrors,
      [],
      `${browserName}/${profileName}: console errors:\n${consoleErrors.join("\n")}`,
    );

    await page.screenshot({ path: path.join(artifactDir, "success.png"), fullPage: true });
    await writeDiagnostics("diagnostics.json", {
      browser: browserName,
      browserVersion: browser.version(),
      profile: profileName,
      runtimeSupported: true,
      missingCapabilities: [],
      capabilities,
      runtime: finalRuntime,
      pageErrors,
      consoleErrors,
    });
    console.log(
      `✓ ${browserName}/${profileName}: ${finalRuntime.rendererBackend} deferred load + public UI select/edit/rerun + resize`,
    );
  }
} catch (error) {
  if (page !== null) {
    try {
      finalRuntime = await runtimeSnapshot(page);
      await page.screenshot({ path: path.join(artifactDir, "failure.png"), fullPage: true });
    } catch {
      // Keep the original failure if the page itself is no longer inspectable.
    }
  }
  await writeDiagnostics("diagnostics.json", {
    browser: browserName,
    browserVersion: browser?.version() ?? null,
    profile: profileName,
    runtimeSupported,
    capabilities,
    runtime: finalRuntime,
    pageErrors,
    consoleErrors,
    error: error instanceof Error ? { name: error.name, message: error.message, stack: error.stack } : String(error),
    serverOutput,
  });
  throw error;
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
