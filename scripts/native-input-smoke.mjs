import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { PNG } from "pngjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4185;
const baseUrl = `http://127.0.0.1:${port}`;

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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
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
    }
  }
  return {
    changedPixels,
    centroidX: weight === 0 ? Number.NaN : weightedX / weight,
  };
}

async function presentCurrent(page) {
  await page.evaluate(() => {
    const player = window.__nativeInputPlayer;
    if (!player.seek(player.time())) throw new Error("native input frame was not presented");
  });
  await page.evaluate(
    () => new Promise((resolve) => requestAnimationFrame(() => resolve())),
  );
}

const source = `
from noon import *

class NativeInputDemo(Scene):
    def construct(self):
        square = Square(side_length=0.9, color=BLUE)
        self.add(square)

        pointer = self.pointer_position_signal()
        self.bind_position(square, pointer)

        visible = self.key_state_signal("Space", False)
        self.bind_presence(square, visible)

        opacity = self.control_signal("opacity", 1.0)
        self.bind_opacity(square, opacity)

        clicks = self.pointer_down_events(0)
        self.bind_rotation(square, clicks)

        self.viewport_size_signal()
        self.wheel_delta_signal()
        self.wheel_events()
        self.control_commit_events("opacity")
`;

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

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());
  const authored = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    source,
  );

  assert.ok(Array.isArray(authored.document.native_inputs));
  const sourceKinds = authored.document.native_inputs.map((binding) => {
    const payload = binding.state ?? binding.event;
    return payload.source.kind;
  });
  for (const kind of [
    "pointer_position",
    "key",
    "control",
    "pointer_down",
    "viewport_size",
    "wheel_delta",
    "wheel",
    "control_commit",
  ]) {
    assert.ok(sourceKinds.includes(kind), `missing authored native input kind ${kind}`);
  }

  const runtimeInfo = await page.evaluate(async (sceneDocument) => {
    const wasm = await import("./pkg/noon_web.js");
    const nativeInputs = await import("./native-inputs.js");
    await wasm.default();

    const canvas = document.createElement("canvas");
    canvas.id = "native-input-canvas";
    canvas.width = 640;
    canvas.height = 360;
    canvas.style.width = "640px";
    canvas.style.height = "360px";
    const slider = document.createElement("input");
    slider.id = "opacity-control";
    slider.type = "range";
    slider.min = "0";
    slider.max = "1";
    slider.step = "0.1";
    slider.value = "1";

    document.body.innerHTML = "";
    document.body.append(canvas, slider);

    const player = await wasm.ReactiveCanvasPlayer.create(
      canvas,
      JSON.stringify(sceneDocument),
      4.0,
    );
    const detachInputs = nativeInputs.attachNativeInputs(player, canvas);
    const detachControl = nativeInputs.bindNativeControl(player, slider, "opacity");
    window.__nativeInputPlayer = player;
    window.__detachNativeInputs = detachInputs;
    window.__detachNativeControl = detachControl;
    if (!player.seek(0.0)) throw new Error("initial native-input frame was not presented");
    return {
      backend: player.rendererBackend(),
      objectCount: player.objectCount(),
      time: player.time(),
    };
  }, authored.document);

  assert.equal(runtimeInfo.backend, "WebGPU");
  assert.equal(runtimeInfo.objectCount, 1);
  assert.equal(runtimeInfo.time, 0);

  const canvas = page.locator("#native-input-canvas");
  const hidden = foregroundStats(await canvas.screenshot());
  assert.ok(
    hidden.changedPixels < 50,
    `key-owned presence should initially hide the object; got ${hidden.changedPixels} pixels`,
  );

  await page.keyboard.down("Space");
  await presentCurrent(page);
  const visible = foregroundStats(await canvas.screenshot());
  assert.ok(
    visible.changedPixels > 200,
    `Space key state should reveal the object; got ${visible.changedPixels} pixels`,
  );

  const box = await canvas.boundingBox();
  assert.ok(box !== null);
  await page.mouse.move(box.x + box.width * 0.78, box.y + box.height * 0.5);
  await presentCurrent(page);
  const moved = foregroundStats(await canvas.screenshot());
  assert.ok(
    moved.centroidX - visible.centroidX > 100,
    `pointer world-position signal should move object right (${visible.centroidX} -> ${moved.centroidX})`,
  );

  await page.mouse.down({ button: "left" });
  await page.mouse.up({ button: "left" });
  await page.mouse.wheel(12, -30);

  await page.locator("#opacity-control").evaluate((element) => {
    element.value = "0.4";
    element.dispatchEvent(new Event("input", { bubbles: true }));
    element.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await presentCurrent(page);

  await page.keyboard.up("Space");
  await presentCurrent(page);
  const hiddenAgain = foregroundStats(await canvas.screenshot());
  assert.ok(
    hiddenAgain.changedPixels < 50,
    `key release should hide the object again; got ${hiddenAgain.changedPixels} pixels`,
  );

  const stats = await page.evaluate(() =>
    JSON.parse(window.__nativeInputPlayer.nativeInputStatsJson()),
  );
  assert.ok(stats.state_samples_received >= 6, JSON.stringify(stats));
  assert.ok(stats.events_received >= 3, JSON.stringify(stats));
  assert.ok(stats.reactive_updates >= 6, JSON.stringify(stats));
  assert.equal(stats.state_dispatches_dropped, 0, JSON.stringify(stats));
  assert.equal(stats.event_dispatches_dropped, 0, JSON.stringify(stats));
  assert.equal(errors.length, 0, errors.join("\n"));

  console.log("native browser input smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
