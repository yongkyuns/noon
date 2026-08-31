import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4193;
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
  throw new Error(`retained family identity smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class RetainedFamilyIdentity(Scene):
    def construct(self):
        first = Text("Family A", font_size=40)
        second = Text("Family B", font_size=40)
        nested = VGroup(first, VGroup(second))

        assert len(nested) == 2
        assert nested[0] is first
        assert len(nested[1]) == 1
        assert nested[1][0] is second

        clone = nested.copy()
        assert len(clone) == 2
        assert clone is not nested
        assert clone[0] is not first
        assert clone[1] is not nested[1]
        assert clone[1][0] is not second

        holder = VGroup(first)
        holder.remove(first)
        assert len(holder) == 0
        holder.add(first)
        assert len(holder) == 1
        assert holder[0] is first
`;

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

  const result = await page.evaluate((python) => window.noonManimCompat.run(python), source);
  assert.equal(result.kind, "scene_document");
  assert.equal(
    result.document.objects.length,
    0,
    "detached retained family identity must not synthesize legacy geometry",
  );
  assert.deepEqual(
    errors,
    [],
    `browser errors while testing retained Text family identity:\n${errors.join("\n")}`,
  );
  console.log(
    "Retained Text family identity smoke passed: nested VGroup construction, copy, remove, and re-add use shared semantic membership without legacy geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
