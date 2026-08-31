import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4197;
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
  throw new Error(`retained text creation smoke server did not start: ${lastError}\n${serverOutput}`);
}

const createUncreateSource = `
from noon import *

class RetainedCreateUncreate(Scene):
    def construct(self):
        label = Text("Create / Uncreate", font_size=72)
        assert label not in self.mobjects

        self.play(Create(label), run_time=1.0, rate_func=linear)
        assert label in self.mobjects

        self.play(Uncreate(label), run_time=1.0, rate_func=linear)
        assert label not in self.mobjects

        self.add(label)
        assert label in self.mobjects
`;

const forwardUncreateSource = `
from noon import *

class RetainedForwardUncreate(Scene):
    def construct(self):
        label = Text("Forward Uncreate", font_size=72)
        self.add(label)
        self.play(
            Uncreate(label, reverse_rate_function=False, remover=False),
            run_time=1.0,
            rate_func=linear,
        )
        assert label in self.mobjects
`;

function scalarTracks(result, propertyName) {
  return (result.retainedDocument?.tracks ?? []).filter(
    (track) => track.property === propertyName,
  );
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
    (source) => window.noonManimCompat.run(source),
    createUncreateSource,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(result.document.objects.length, 0, "Text creation must remain retained-only");
  assert.equal(result.retainedDocument.objects.length, 1);
  assert.equal(result.retainedDocument.objects[0].text.source, "Create / Uncreate");
  assert.equal(result.duration, 2);

  const reveals = scalarTracks(result, "reveal");
  assert.equal(reveals.length, 3, "Create, Uncreate, and removal cleanup must author reveal tracks");
  assert.deepEqual(
    reveals.map((track) => ({ values: track.values.scalar, timing: track.timing })),
    [
      {
        values: { from: 0, to: 1 },
        timing: { start_time: 0, duration: 1, easing: "linear" },
      },
      {
        values: { from: 1, to: 0 },
        timing: { start_time: 1, duration: 1, easing: "linear" },
      },
      {
        values: { from: 0, to: 1 },
        timing: { start_time: 2, duration: 0, easing: "linear" },
      },
    ],
  );

  const presence = (result.retainedDocument.tracks ?? []).filter(
    (track) => track.property === "presence",
  );
  assert.deepEqual(
    presence.map((track) => ({ values: track.values.bool, timing: track.timing })),
    [
      {
        values: { from: true, to: false },
        timing: { start_time: 2, duration: 0, easing: "linear" },
      },
      {
        values: { from: false, to: true },
        timing: { start_time: 2, duration: 0, easing: "linear" },
      },
    ],
    "Uncreate removes at the exact end and Scene.add reintroduces the canonical Text",
  );

  const forwardResult = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    forwardUncreateSource,
  );
  const forwardReveal = scalarTracks(forwardResult, "reveal");
  assert.equal(forwardReveal.length, 1);
  assert.deepEqual(forwardReveal[0].values.scalar, { from: 0, to: 1 });
  assert.deepEqual(forwardReveal[0].timing, {
    start_time: 0,
    duration: 1,
    easing: "linear",
  });
  assert.equal(
    (forwardResult.retainedDocument.tracks ?? []).filter(
      (track) => track.property === "presence",
    ).length,
    0,
    "remover=False must keep the retained Text present",
  );

  assert.deepEqual(errors, []);
  console.log("retained Text Create/Uncreate authoring smoke passed");
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
