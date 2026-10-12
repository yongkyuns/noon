import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4183;
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
  throw new Error(`SVG authoring smoke server did not start: ${lastError}\n${serverOutput}`);
}

const svgSource = String.raw`
from noon import *

class SvgStatic(Scene):
    def construct(self):
        icon = SVGMobject.from_string('''
<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
  <rect x="0" y="0" width="10" height="10" fill="#ff0000"/>
  <path d="M 10 0 L 20 0 L 20 10 Z" fill="#00ff00"/>
</svg>
''')
        assert isinstance(icon, VGroup)
        assert len(icon) == 2
        assert abs(icon.height - 2.0) < 1e-6
        center = icon.get_center()
        assert abs(center[0]) < 1e-6
        assert abs(center[1]) < 1e-6
        self.add(icon)
`;

const unsupportedSource = String.raw`
from noon import *

class UnsupportedSvg(Scene):
    def construct(self):
        SVGMobject.from_string('''
<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <defs><linearGradient id="g"><stop offset="0" stop-color="red"/></linearGradient></defs>
  <rect width="10" height="10" fill="url(#g)"/>
</svg>
''')
`;

let browser;
try {
  await waitForServer();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`);
  await page.evaluate(() => window.noonManimCompat.ready());

  const rendered = await page.evaluate(
    (source) => window.noonManimCompat.runLive(source),
    svgSource,
  );
  assert.equal(rendered.duration, 0, "static SVG authoring must not invent timeline duration");
  assert.equal(rendered.mode, "semantic", "SVG must use shared semantic retained execution");
  assert.equal(rendered.metrics.objectCount, 2, "SVG family must render both retained leaves");
  assert.ok(rendered.metrics.presentedFrames > 0, "SVG retained scene must present at least one frame");

  let unsupportedError = null;
  try {
    await page.evaluate(
      (source) => window.noonManimCompat.runLive(source),
      unsupportedSource,
    );
  } catch (error) {
    unsupportedError = String(error);
  }
  assert.match(
    unsupportedError ?? "",
    /svg\.unsupported|unsupported SVG element <linearGradient>/,
    "unsupported SVG semantics must fail explicitly through the typed authoring boundary",
  );

  assert.deepEqual(errors, [], `browser errors while testing SVG authoring:\n${errors.join("\n")}`);
  console.log(
    "SVG authoring smoke passed: public SVGMobject import, retained family/leaf identity, Manim default placement, static browser rendering, and explicit unsupported-feature failure.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
