import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { PNG } from "pngjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4185;
const baseUrl = `http://127.0.0.1:${port}`;
const source = await readFile(
  path.join(repoRoot, "web/python/examples/live_native_signals.py"),
  "utf8",
);

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/execution-worker-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Native input smoke server did not start: ${lastError}\n${serverOutput}`);
}

function foregroundStats(buffer) {
  const image = PNG.sync.read(buffer);
  const background = [image.data[0], image.data[1], image.data[2], image.data[3]];
  let weightedX = 0;
  let weight = 0;
  let changedPixels = 0;
  let blue = 0;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      const distance =
        Math.abs(image.data[offset] - background[0]) +
        Math.abs(image.data[offset + 1] - background[1]) +
        Math.abs(image.data[offset + 2] - background[2]) +
        Math.abs(image.data[offset + 3] - background[3]);
      if (distance < 32) continue;
      changedPixels += 1;
      weightedX += x * distance;
      weight += distance;
      blue += image.data[offset + 2];
    }
  }
  return {
    changedPixels,
    centroidX: weight === 0 ? Number.NaN : weightedX / weight,
    meanBlue: changedPixels === 0 ? 0 : blue / changedPixels,
  };
}

function differingPixels(beforeBuffer, afterBuffer) {
  const before = PNG.sync.read(beforeBuffer);
  const after = PNG.sync.read(afterBuffer);
  assert.equal(before.width, after.width);
  assert.equal(before.height, after.height);
  let differing = 0;
  for (let offset = 0; offset < before.data.length; offset += 4) {
    const distance =
      Math.abs(before.data[offset] - after.data[offset]) +
      Math.abs(before.data[offset + 1] - after.data[offset + 1]) +
      Math.abs(before.data[offset + 2] - after.data[offset + 2]) +
      Math.abs(before.data[offset + 3] - after.data[offset + 3]);
    if (distance >= 32) differing += 1;
  }
  return differing;
}

async function waitForPresentedFrame(page, afterPresentedFrames, objectCount) {
  return page.evaluate(async ({ afterPresentedFrames, objectCount }) => {
    const execution = window.__nativeInputExecution;
    let latest;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      latest = (await execution.metrics()).metrics;
      if (
        latest.presentedFrames > afterPresentedFrames &&
        latest.objectCount === objectCount &&
        (objectCount === 0 || latest.drawCalls > 0)
      ) return latest;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    throw new Error(`native input did not render: ${JSON.stringify(latest)}`);
  }, { afterPresentedFrames, objectCount });
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--enable-unsafe-webgpu",
      "--enable-unsafe-swiftshader",
      "--use-webgpu-adapter=swiftshader",
      "--use-gpu-in-tests",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-gl=angle",
      "--use-angle=swiftshader",
      "--use-vulkan=swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });

  const page = await browser.newPage({ viewport: { width: 760, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });

  const runtimeInfo = await page.evaluate(async (pythonSource) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const {
      attachNativeInputs,
      bindNativeControl,
      createExecutionWorkerNativeInputHost,
    } = await import("./native-inputs.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    const authored = await authoring.run(pythonSource, {});
    if ("document" in authored || "sceneSpec" in authored) {
      throw new Error("canonical native input scene exported legacy state");
    }

    const canvas = document.querySelector("#scene");
    const slider = document.createElement("input");
    slider.id = "opacity-control";
    slider.type = "range";
    slider.min = "0";
    slider.max = "1";
    slider.step = "0.1";
    slider.value = "1";
    document.body.append(slider);

    const inputErrors = [];
    const recordError = (error) => inputErrors.push(String(error));
    const execution = new AuthoringExecutionClient(canvas, { onError: recordError });
    const ready = await execution.startSemanticExecution(authored.semanticExecution, {
      authoringClient: authoring,
      transportMode: "transferable",
    });
    let initial;
    for (let attempt = 0; attempt < 150; attempt += 1) {
      initial = (await execution.metrics()).metrics;
      if (initial.presentedFrames > 0 && initial.objectCount === 0) break;
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    if (!(initial?.presentedFrames > 0) || initial.objectCount !== 0) {
      throw new Error(`native input initial frame did not stay hidden: ${JSON.stringify(initial)}`);
    }
    const paused = await execution.pause();
    if (paused.playing) throw new Error("native input execution did not pause");

    // This fixture uses the default canonical viewport. A split-worker product
    // host supplies its platform viewport mapping through the same adapter.
    const host = createExecutionWorkerNativeInputHost(execution, {
      pointerToScene(normalizedX, normalizedY) {
        const frameHeight = 8;
        const frameWidth = frameHeight * (canvas.width / canvas.height);
        return {
          x: (normalizedX - 0.5) * frameWidth,
          y: (0.5 - normalizedY) * frameHeight,
        };
      },
    });
    const detachInputs = attachNativeInputs(host, canvas, { onError: recordError });
    const detachControl = bindNativeControl(host, slider, "opacity", { onError: recordError });
    window.__nativeInputExecution = execution;
    window.__nativeInputCleanup = () => {
      detachInputs();
      detachControl();
      execution.terminate();
      authoring.terminate();
    };
    window.__nativeInputErrors = inputErrors;
    return {
      backend: ready.render.backend,
      objectCount: initial.objectCount,
      presentedFrames: initial.presentedFrames,
    };
  }, source);

  assert.equal(runtimeInfo.backend, "WebGPU");
  assert.equal(runtimeInfo.objectCount, 0);
  const canvas = page.locator("#scene");
  const hidden = foregroundStats(await canvas.screenshot());
  assert.ok(hidden.changedPixels < 50, "Space=false must keep the native square hidden");

  await page.keyboard.down("Space");
  const visibleMetrics = await waitForPresentedFrame(page, runtimeInfo.presentedFrames, 1);
  const visibleBuffer = await canvas.screenshot();
  const visible = foregroundStats(visibleBuffer);
  assert.ok(visible.changedPixels > 200, "Space key state did not reveal the square");

  const box = await canvas.boundingBox();
  assert.ok(box !== null);
  await page.mouse.move(box.x + box.width * 0.78, box.y + box.height * 0.5);
  const movedMetrics = await waitForPresentedFrame(page, visibleMetrics.presentedFrames, 1);
  const movedBuffer = await canvas.screenshot();
  const moved = foregroundStats(movedBuffer);
  assert.ok(
    moved.centroidX - visible.centroidX > 100,
    `normalized pointer did not move the square through the canonical camera (${visible.centroidX} -> ${moved.centroidX})`,
  );

  await page.mouse.down({ button: "left" });
  const rotatedMetrics = await waitForPresentedFrame(page, movedMetrics.presentedFrames, 1);
  const rotatedBuffer = await canvas.screenshot();
  assert.ok(
    differingPixels(movedBuffer, rotatedBuffer) > 100,
    "ordered pointer-down event did not rotate the square",
  );

  await page.locator("#opacity-control").evaluate((element) => {
    element.value = "0.4";
    element.dispatchEvent(new Event("input", { bubbles: true }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
  });
  const dimmedMetrics = await waitForPresentedFrame(page, rotatedMetrics.presentedFrames, 1);
  const dimmed = foregroundStats(await canvas.screenshot());
  const rotated = foregroundStats(rotatedBuffer);
  assert.ok(
    dimmed.meanBlue < rotated.meanBlue * 0.75,
    `native control did not dim the square (${rotated.meanBlue} -> ${dimmed.meanBlue})`,
  );

  await page.keyboard.up("Space");
  await waitForPresentedFrame(page, dimmedMetrics.presentedFrames, 0);
  const hiddenAgain = foregroundStats(await canvas.screenshot());
  assert.ok(hiddenAgain.changedPixels < 50, "Space key release did not hide the square");

  const inputErrors = await page.evaluate(() => window.__nativeInputErrors.slice());
  assert.deepEqual(inputErrors, []);
  assert.deepEqual(errors, []);
  await page.evaluate(() => window.__nativeInputCleanup());
  console.log("canonical native browser input smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
