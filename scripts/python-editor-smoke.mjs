import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4174;
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
      const response = await fetch(`${baseUrl}/web/index.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Python editor smoke server did not start: ${lastError}\n${serverOutput}`);
}

let browser = null;
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

  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/index.html`, { waitUntil: "load" });
  await page.waitForSelector(".python-code-editor[data-editor-ready='true'] .cm-editor", {
    timeout: 30_000,
  });
  assert.equal(
    await page.locator(".python-code-editor[data-editor-ready='true']").count(),
    2,
    "both Python textareas should be enhanced",
  );

  await page.waitForFunction(
    () => document.querySelector("#python-scene-source")?.value.includes("from noon import"),
    null,
    { timeout: 30_000 },
  );
  const highlightedSpans = await page.locator("#scene-editor-panel .cm-line span").count();
  assert.ok(highlightedSpans > 0, "Python source should contain syntax-highlighted spans");

  let runtimeStatus = null;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    runtimeStatus = await page.evaluate(() => {
      const status = document.querySelector("#status");
      return {
        state: status?.dataset.state ?? null,
        text: document.querySelector("#status-text")?.textContent ?? status?.textContent ?? "",
      };
    });
    if (runtimeStatus.state === "running" || runtimeStatus.state === "error") {
      break;
    }
    await page.waitForTimeout(500);
  }
  assert.equal(
    runtimeStatus?.state,
    "running",
    `playground runtime did not become ready: ${JSON.stringify(runtimeStatus)}\n${errors.join("\n")}`,
  );

  const layout = await page.evaluate(() => {
    const scroller = document
      .querySelector("#scene-editor-panel .cm-scroller")
      .getBoundingClientRect();
    const content = document
      .querySelector("#scene-editor-panel .cm-content")
      .getBoundingClientRect();
    const wrap = document.querySelector(".canvas-wrap").getBoundingClientRect();
    const canvas = document.querySelector("#scene").getBoundingClientRect();
    return {
      editorTopInset: content.top - scroller.top,
      canvasTopGap: canvas.top - wrap.top,
      canvasBottomGap: wrap.bottom - canvas.bottom,
      canvasLeftGap: canvas.left - wrap.left,
      canvasRightGap: wrap.right - canvas.right,
    };
  });
  assert.ok(
    layout.editorTopInset <= 8,
    `CodeMirror should start near the top of its pane; got ${layout.editorTopInset}px`,
  );
  for (const [name, gap] of Object.entries({
    canvasTopGap: layout.canvasTopGap,
    canvasBottomGap: layout.canvasBottomGap,
    canvasLeftGap: layout.canvasLeftGap,
    canvasRightGap: layout.canvasRightGap,
  })) {
    assert.ok(Math.abs(gap) <= 2, `desktop preview should fill its pane; ${name}=${gap}px`);
  }

  await page.evaluate(() => {
    document.querySelector("#python-scene-source").value =
      "import os\n\ndef broken():\n    return missing_name\n";
  });
  await page.waitForSelector("#scene-editor-panel .cm-lintRange-warning", {
    timeout: 30_000,
  });
  const lintRanges = await page.locator("#scene-editor-panel .cm-lintRange-warning").count();
  assert.ok(lintRanges >= 1, "Ruff should report inline Python diagnostics");

  assert.deepEqual(errors, [], `browser errors while loading Python editor:\n${errors.join("\n")}`);
  console.log(
    `Python editor smoke passed: tight desktop layout, 2 CodeMirror editors, syntax highlighting, ${lintRanges} Ruff diagnostics.`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
