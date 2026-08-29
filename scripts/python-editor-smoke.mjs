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

async function waitForRuntime(page, errors, label) {
  let runtimeStatus = null;
  for (let attempt = 0; attempt < 120; attempt += 1) {
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
    `${label}: playground runtime did not become ready: ${JSON.stringify(runtimeStatus)}\n${errors.join("\n")}`,
  );
}

async function waitForAppliedScene(page, errors, label, expectedText = null) {
  await page.waitForFunction(
    (needle) => {
      const patch = document.querySelector("#patch-status");
      const text = patch?.value ?? patch?.textContent ?? "";
      if (patch?.dataset.state === "error") return true;
      return patch?.dataset.state === "applied" && (needle === null || text.includes(needle));
    },
    expectedText,
    { timeout: 60_000 },
  );
  const patch = await page.evaluate(() => ({
    state: document.querySelector("#patch-status")?.dataset.state ?? null,
    text:
      document.querySelector("#patch-status")?.value ??
      document.querySelector("#patch-status")?.textContent ??
      "",
  }));
  assert.equal(
    patch.state,
    "applied",
    `${label}: authored scene did not apply: ${JSON.stringify(patch)}\n${errors.join("\n")}`,
  );
  if (expectedText !== null) {
    assert.match(patch.text, new RegExp(expectedText.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
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
  await page.waitForFunction(
    () =>
      document.querySelector("#python-scene-source")?.value.includes("from noon import") ||
      document.querySelector("#status")?.dataset.state === "error",
    null,
    { timeout: 30_000 },
  );
  const startup = await page.evaluate(() => ({
    sourceLoaded:
      document.querySelector("#python-scene-source")?.value.includes("from noon import") ?? false,
    state: document.querySelector("#status")?.dataset.state ?? null,
    runtimeStartup: document.querySelector("#status")?.dataset.runtimeStartup ?? null,
    text:
      document.querySelector("#status-text")?.textContent ??
      document.querySelector("#status")?.textContent ??
      "",
  }));
  assert.equal(
    startup.sourceLoaded,
    true,
    `playground failed before loading editor source: ${JSON.stringify(startup)}\n${errors.join("\n")}`,
  );
  assert.equal(startup.runtimeStartup, "deferred", "page load must not start the execution runtime");
  assert.equal(
    await page.locator(".python-code-editor").count(),
    0,
    "CodeMirror/Ruff must remain unloaded until the source editor is focused",
  );

  await page.locator("#python-scene-source").focus();
  await page.waitForSelector(".python-code-editor[data-editor-ready='true'] .cm-editor", {
    timeout: 30_000,
  });
  assert.equal(
    await page.locator(".python-code-editor[data-editor-ready='true']").count(),
    2,
    "both Python textareas should be enhanced after the first editor focus",
  );

  const highlightedSpans = await page.locator("#scene-editor-panel .cm-line span").count();
  assert.ok(highlightedSpans > 0, "Python source should contain syntax-highlighted spans");

  await page.locator("#replace-scene").click();
  await waitForRuntime(page, errors, "enhanced editor");
  await waitForAppliedScene(page, errors, "enhanced editor");

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
      canvasWidth: canvas.width,
    };
  });
  assert.ok(
    layout.editorTopInset <= 8,
    `CodeMirror should start near the top of its pane; got ${layout.editorTopInset}px`,
  );
  assert.ok(
    layout.canvasWidth <= 44 * 16 + 1,
    `desktop preview should respect the 44rem viewport cap; got ${layout.canvasWidth}px`,
  );
  assert.ok(
    layout.canvasTopGap >= 0 && layout.canvasBottomGap >= 0,
    `desktop preview must stay within its wrapper vertically: ${JSON.stringify(layout)}`,
  );
  assert.ok(
    layout.canvasLeftGap >= 0 && layout.canvasRightGap >= 0,
    `desktop preview must stay within its wrapper horizontally: ${JSON.stringify(layout)}`,
  );
  assert.ok(
    Math.abs(layout.canvasTopGap - layout.canvasBottomGap) <= 2,
    `desktop preview should remain vertically centered; ${JSON.stringify(layout)}`,
  );
  assert.ok(
    Math.abs(layout.canvasLeftGap - layout.canvasRightGap) <= 2,
    `desktop preview should remain horizontally centered; ${JSON.stringify(layout)}`,
  );

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
  await page.close();

  const fallbackContext = await browser.newContext({ viewport: { width: 1200, height: 800 } });
  await fallbackContext.route("https://esm.sh/**", (route) => route.abort("failed"));
  const fallbackPage = await fallbackContext.newPage();
  const fallbackErrors = [];
  const fallbackWarnings = [];
  fallbackPage.on("pageerror", (error) => fallbackErrors.push(`pageerror: ${error}`));
  fallbackPage.on("console", (message) => {
    if (message.type() === "warning") fallbackWarnings.push(message.text());
    if (
      message.type() === "error" &&
      !message.text().includes("ERR_FAILED") &&
      !message.text().includes("esm.sh")
    ) {
      fallbackErrors.push(`console: ${message.text()}`);
    }
  });

  await fallbackPage.goto(`${baseUrl}/web/index.html?example=parity-create-circle`, {
    waitUntil: "load",
  });
  await fallbackPage.waitForFunction(
    () => document.querySelector("#python-scene-source")?.value.includes("from noon import"),
    null,
    { timeout: 30_000 },
  );
  const enhancementWarning = fallbackPage.waitForEvent("console", {
    predicate: (message) =>
      message.type() === "warning" &&
      message.text().includes("Enhanced Python editor unavailable; using textarea fallback"),
    timeout: 30_000,
  });
  await fallbackPage.locator("#python-scene-source").focus();
  await enhancementWarning;

  const fallback = await fallbackPage.evaluate(() => {
    const textarea = document.querySelector("#python-scene-source");
    return {
      textareaHidden: textarea?.hidden ?? true,
      editorCount: document.querySelectorAll(".python-code-editor").length,
      source: textarea?.value ?? "",
      runtimeStartup: document.querySelector("#status")?.dataset.runtimeStartup ?? null,
    };
  });
  assert.equal(fallback.textareaHidden, false, "CDN failure must keep the native textarea visible");
  assert.equal(fallback.editorCount, 0, "failed enhancement must not leave partial editor hosts");
  assert.match(fallback.source, /from noon import \*/);
  assert.equal(fallback.runtimeStartup, "deferred", "editor fallback must not start the runtime");

  await fallbackPage.locator("#replace-scene").click();
  await waitForRuntime(fallbackPage, fallbackErrors, "textarea fallback");
  await waitForAppliedScene(fallbackPage, fallbackErrors, "textarea fallback");
  assert.equal(
    await fallbackPage.locator("#status").getAttribute("data-renderer-backend"),
    "WebGL2",
  );

  await fallbackPage.locator("#python-scene-source").fill(
    "from noon import *\n\nclass FallbackScene(Scene):\n    def construct(self):\n        self.add(Square())\n        self.add(Circle().shift(RIGHT * 2))\n",
  );
  await fallbackPage.locator("#replace-scene").click();
  await waitForAppliedScene(
    fallbackPage,
    fallbackErrors,
    "edited textarea fallback",
    "2 objects",
  );
  assert.ok(
    fallbackWarnings.some((warning) => warning.includes("Enhanced Python editor unavailable")),
    `expected a non-fatal enhancement warning; got ${fallbackWarnings.join("\n")}`,
  );
  assert.deepEqual(
    fallbackErrors,
    [],
    `editor CDN failure must not emit unhandled browser errors:\n${fallbackErrors.join("\n")}`,
  );
  await fallbackContext.close();

  console.log(
    `Python editor smoke passed: deferred startup + enhanced editor + ${lintRanges} Ruff diagnostic(s) + CDN-blocked textarea fallback with a fresh two-object render.`,
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
