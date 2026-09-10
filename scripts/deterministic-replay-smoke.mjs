import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = Number(process.env.NOON_DETERMINISM_PORT ?? "4183");
const baseUrl = `http://127.0.0.1:${port}`;
const forwardSampleCount = 32;
const stressCount = Number(process.env.NOON_DETERMINISM_STRESS_COUNT ?? "1000");
assert.ok(Number.isInteger(stressCount) && stressCount >= 1 && stressCount <= 100_000,
  "NOON_DETERMINISM_STRESS_COUNT must be an integer in 1..100000");
const examples = [
  "exact-property-tracks", "specialized-geometry", "family-placement",
  "painter-order", "analytic-stress", "create-morph-fade",
];
const targets = [0, 0.25, 0.5, 0.999, 1, 1.001, 1.5, 2, 2.5, 3, 3.75];

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
    if ("verifySceneReplay" in wasm) throw new Error("legacy scene-document replay API returned");
    if (typeof wasm.verifyDirectExecutionReplay !== "function") {
      throw new Error("Replay qualification requires a dev package or the explicit replay-smoke release feature; production packages omit fixtures");
    }
    window.noonDeterminism = { verify: wasm.verifyDirectExecutionReplay };
  });

  for (const example of examples) {
    await page.evaluate(
      ({ example, targetTimes, sampleCount, count }) => {
        window.noonDeterminism.verify(example, new Float64Array(targetTimes), sampleCount, count);
      },
      { example, targetTimes: targets, sampleCount: forwardSampleCount, count: stressCount },
    );
    console.log(`✓ ${example}: typed direct/playback/rewind WASM state agrees`);
  }

  assert.deepEqual(browserErrors, [], `unexpected browser errors:\n${browserErrors.join("\n")}`);
  console.log(
    `Deterministic replay smoke passed for ${examples.length} shared Rust scenes ` +
      `with ${forwardSampleCount} forward samples per target.`,
  );
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
