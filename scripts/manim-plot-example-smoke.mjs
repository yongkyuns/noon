import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const fixturePath = path.join(
  repoRoot,
  "parity/manim-v0.21/upstream-examples/plot_example.py",
);
const port = 4193;
const baseUrl = `http://127.0.0.1:${port}`;

const upstreamSource = await readFile(fixturePath, "utf8");
const importLine = "from manim import *";
assert.equal(
  upstreamSource.split(importLine).length - 1,
  1,
  "pinned PlotExample must contain exactly one Manim wildcard import",
);
assert.ok(
  upstreamSource.startsWith(`${importLine}\n`),
  "pinned PlotExample must preserve the upstream import as its first line",
);
const noonSource = upstreamSource.replace(importLine, "from noon import *");
assert.equal(
  noonSource.replace("from noon import *", importLine),
  upstreamSource,
  "PlotExample execution may change only the import line",
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
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`PlotExample smoke server did not start: ${lastError}\n${serverOutput}`);
}

let browser = null;
try {
  await waitForServer();
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  await page.evaluate(() => window.noonManimCompat.ready());

  const result = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    noonSource,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.tracks.length, 0);
  assert.equal(
    result.document.objects.length,
    57,
    "three PlotExample Axes should flatten to 54 line/tick leaves plus three curves",
  );

  const geometryKinds = result.document.objects.map(
    (object) => Object.keys(object.geometry)[0],
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "line").length,
    54,
    "PlotExample Axes must remain ordinary retained line/tick leaves",
  );
  assert.equal(
    geometryKinds.filter((kind) => kind === "vector_path").length,
    3,
    "all three upstream log plots must lower to ordinary retained VectorPath curves",
  );
  assert.deepEqual(
    errors,
    [],
    `browser errors while executing exact upstream PlotExample:\n${errors.join("\n")}`,
  );
  console.log(
    "Exact Manim v0.21 PlotExample passed: import-only substitution, copied/placed Axes, lazy np.log, pure colors, smoothing modes, explicit dense sampling, and retained VectorPath output.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
