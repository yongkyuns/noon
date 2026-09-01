import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtemp, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const sceneDir = await mkdtemp(path.join(tmpdir(), "noon-determinism-scenes-"));
const port = Number(process.env.NOON_DETERMINISM_PORT ?? "4183");
const baseUrl = `http://127.0.0.1:${port}`;
const forwardSampleCount = 32;
const morphStressCount = process.env.NOON_DETERMINISM_MORPH_STRESS_COUNT;
const generatorArgs = ["web/python/playground_examples.py", sceneDir];
if (morphStressCount !== undefined) {
  if (!/^\d+$/.test(morphStressCount) || Number(morphStressCount) < 12) {
    throw new Error("NOON_DETERMINISM_MORPH_STRESS_COUNT must be an integer >= 12");
  }
  generatorArgs.push("--morph-stress-count", morphStressCount);
}

const generated = spawnSync("python3", generatorArgs, {
  cwd: repoRoot,
  encoding: "utf8",
  env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
});
if (generated.status !== 0) {
  throw new Error(`Unable to generate deterministic scenes:\n${generated.stdout}\n${generated.stderr}`);
}
const examples = generated.stdout
  .trim()
  .split("\n")
  .filter(Boolean)
  .map((line) => {
    const separator = line.indexOf("\t");
    assert.notEqual(separator, -1, `unexpected playground generator output: ${line}`);
    return { name: line.slice(0, separator), file: line.slice(separator + 1) };
  });
assert.ok(examples.length > 0, "determinism corpus must contain playground scenes");

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
  throw new Error(`determinism smoke server did not start: ${lastError}\n${serverOutput}`);
}

function sceneEnd(document) {
  if (document.tracks.length === 0) return 0;
  return Math.max(
    ...document.tracks.map((track) => track.timing.start_time + track.timing.duration),
  );
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({ channel: "chromium", headless: true });
  const page = await browser.newPage();
  const browserErrors = [];
  page.on("pageerror", (error) => browserErrors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(`console: ${message.text()}`);
  });
  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });

  await page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    window.noonDeterminism = { verify: wasm.verifySceneReplay };
  });

  for (const example of examples) {
    const sceneJson = await readFile(example.file, "utf8");
    const document = JSON.parse(sceneJson);
    const end = sceneEnd(document);
    const targets =
      end > 0 ? [0, end * 0.125, end * 0.5, end * 0.875, end, end + 0.75] : [0, 0.5];

    await page.evaluate(
      ({ json, targetTimes, sampleCount }) => {
        window.noonDeterminism.verify(json, JSON.stringify(targetTimes), sampleCount);
      },
      { json: sceneJson, targetTimes: targets, sampleCount: forwardSampleCount },
    );
    console.log(`✓ ${example.name}: direct/playback/rewind WASM snapshots agree`);
  }

  assert.deepEqual(browserErrors, [], `unexpected browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `Deterministic replay smoke passed for ${examples.length} playground scenes ` +
      `with ${forwardSampleCount} forward samples per target.`,
  );
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
