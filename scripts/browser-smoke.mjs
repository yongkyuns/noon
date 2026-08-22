import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, mkdir, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import pngjs from "pngjs";

const { chromium } = playwright;
const { PNG } = pngjs;

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const sceneDir = await mkdtemp(path.join(tmpdir(), "noon-browser-scenes-"));
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_BROWSER_SMOKE_ARTIFACTS ?? "browser-smoke-artifacts",
);
const port = Number(process.env.NOON_BROWSER_SMOKE_PORT ?? "4173");
const baseUrl = `http://127.0.0.1:${port}`;

await mkdir(artifactDir, { recursive: true });

const generated = spawnSync(
  "python3",
  ["web/python/playground_examples.py", sceneDir],
  {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      PYTHONDONTWRITEBYTECODE: "1",
    },
  },
);
if (generated.status !== 0) {
  throw new Error(
    `Unable to generate playground scenes:\n${generated.stdout}\n${generated.stderr}`,
  );
}

const examples = generated.stdout
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((line) => {
    const separator = line.indexOf("\t");
    if (separator === -1) {
      throw new Error(`Unexpected playground generator output: ${line}`);
    }
    return {
      name: line.slice(0, separator),
      file: line.slice(separator + 1),
    };
  });

assert.equal(examples.length, 10, "browser smoke must cover every picker scene");

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  {
    cwd: repoRoot,
    stdio: ["ignore", "pipe", "pipe"],
  },
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
      const response = await fetch(`${baseUrl}/web/browser-smoke.html`);
      if (response.ok) {
        return;
      }
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Browser smoke server did not start: ${lastError}\n${serverOutput}`);
}

function artifactName(index, name) {
  const slug = name
    .normalize("NFKD")
    .replace(/[^a-zA-Z0-9]+/g, "-")
    .replace(/^-|-$/g, "")
    .toLowerCase();
  return `${String(index).padStart(2, "0")}-${slug || "scene"}.png`;
}

function assertVisiblePixels(buffer, name) {
  const png = PNG.sync.read(buffer);
  assert.ok(png.width >= 320 && png.height >= 180, `${name}: canvas screenshot is too small`);

  const background = [png.data[0], png.data[1], png.data[2], png.data[3]];
  let changedPixels = 0;
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const distance =
      Math.abs(png.data[offset] - background[0]) +
      Math.abs(png.data[offset + 1] - background[1]) +
      Math.abs(png.data[offset + 2] - background[2]) +
      Math.abs(png.data[offset + 3] - background[3]);
    if (distance >= 32) {
      changedPixels += 1;
    }
  }

  assert.ok(
    changedPixels >= 100,
    `${name}: captured canvas appears blank (${changedPixels} non-background pixels)`,
  );
  return changedPixels;
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: [
      "--enable-unsafe-webgpu",
      "--ignore-gpu-blocklist",
      "--enable-features=Vulkan",
      "--use-angle=vulkan",
      "--use-vulkan=swiftshader",
      "--enable-unsafe-swiftshader",
      "--disable-gpu-sandbox",
      "--disable-dev-shm-usage",
    ],
  });

  const page = await browser.newPage({ viewport: { width: 1000, height: 600 } });
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") {
      browserErrors.push(`console: ${message.text()}`);
    }
  });

  await page.goto(`${baseUrl}/web/browser-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonSmoke?.state.ready === true, null, {
    timeout: 30_000,
  });

  const initial = await page.evaluate(() => window.noonSmoke.metrics());
  if (initial.error) {
    throw new Error(`WebGPU harness failed to initialize: ${initial.error}`);
  }

  for (const [index, example] of examples.entries()) {
    const sceneJson = await readFile(example.file, "utf8");
    const document = JSON.parse(sceneJson);
    const expectedObjects = document.objects.length;
    assert.ok(expectedObjects > 0, `${example.name}: scene has no semantic objects`);

    const loaded = await page.evaluate(
      (json) => window.noonSmoke.loadScene(json),
      sceneJson,
    );
    assert.equal(
      loaded.objectCount,
      expectedObjects,
      `${example.name}: browser object count after load`,
    );

    await page.waitForFunction(
      ({ revision, objectCount }) => {
        const metrics = window.noonSmoke.metrics();
        if (metrics.error) {
          throw new Error(metrics.error);
        }
        return (
          metrics.revision === revision &&
          metrics.objectCount === objectCount &&
          metrics.framesSinceLoad >= 4 &&
          metrics.time >= 0.45 &&
          metrics.drawCalls > 0 &&
          metrics.instances > 0
        );
      },
      { revision: loaded.revision, objectCount: expectedObjects },
      { timeout: 20_000 },
    );

    const metrics = await page.evaluate(() => window.noonSmoke.metrics());
    assert.equal(metrics.error, null, `${example.name}: browser runtime error`);
    assert.equal(metrics.objectCount, expectedObjects, `${example.name}: object count drifted`);
    assert.ok(metrics.drawCalls > 0, `${example.name}: renderer emitted no draw calls`);
    assert.ok(metrics.instances > 0, `${example.name}: renderer emitted no instances`);

    const screenshotPath = path.join(artifactDir, artifactName(index, example.name));
    const screenshot = await page.locator("#scene").screenshot({ path: screenshotPath });
    const visiblePixels = assertVisiblePixels(screenshot, example.name);

    console.log(
      `✓ ${example.name}: ${metrics.objectCount} objects, ${metrics.drawCalls} draws, ` +
        `${metrics.instances} instances, ${visiblePixels} visible pixels`,
    );
  }

  assert.deepEqual(browserErrors, [], `browser emitted errors:\n${browserErrors.join("\n")}`);
  console.log(`Browser WebGPU smoke passed for ${examples.length} picker scenes.`);
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
