import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4180;
const baseUrl = `http://127.0.0.1:${port}`;

const pythonCases = {
  animate_options: `
from noon import *

class ParityAnimate(Scene):
    def construct(self):
        circle = Circle(radius=1.0)
        self.add(circle)
        self.play(
            circle.animate.shift(RIGHT).rotate(0.25),
            run_time=2.0,
            rate_func=smooth,
        )
`,
  lifecycle: `
from noon import *

class ParityLifecycle(Scene):
    def construct(self):
        circle = Circle(radius=0.5)
        self.play(Create(circle), run_time=1.0)
        self.play(FadeOut(circle), run_time=0.5)
        self.play(FadeIn(circle), run_time=0.5)
`,
  nonlinear_composition: `
from noon import *

class ParityComposition(Scene):
    def construct(self):
        circle = Circle(radius=0.4)
        square = Square(side_length=0.8)
        self.add(circle, square)
        group = AnimationGroup(
            circle.animate.shift(UP),
            square.animate.shift(DOWN),
            lag_ratio=0.5,
            rate_func=there_and_back,
        )
        self.play(group, run_time=3.0)
`,
};

const movingAroundPython = readFileSync(
  path.join(repoRoot, "web", "python", "examples", "manim_gallery_moving_around.py"),
  "utf8",
);
const javascriptParityCases = new Set([
  "animate_options",
  "lifecycle",
]);

const quickstartEquivalentCases = new Set([
  "CreateCircle",
  "SquareToCircle",
  "SquareAndCircle",
  "AnimatedSquareToCircle",
  "DifferentRotations",
]);

function rustCorpus(example = "cross_language_parity") {
  const output = execFileSync(
    "cargo",
    ["run", "--quiet", "-p", "noon", "--example", example],
    { cwd: repoRoot, encoding: "utf8" },
  );
  const corpus = new Map();
  for (const line of output.trim().split("\n")) {
    const separator = line.indexOf("\t");
    assert.ok(separator > 0, `invalid Rust parity corpus line: ${line}`);
    corpus.set(line.slice(0, separator), JSON.parse(line.slice(separator + 1)));
  }
  return corpus;
}

function assertSemanticEqual(actual, expected, location = "document") {
  if (typeof actual === "number" && typeof expected === "number") {
    const normalizedActual = Object.is(actual, -0) ? 0 : actual;
    const normalizedExpected = Object.is(expected, -0) ? 0 : expected;
    assert.ok(
      Math.abs(normalizedActual - normalizedExpected) <= 1e-6,
      `${location}: ${normalizedActual} != ${normalizedExpected}`,
    );
    return;
  }
  if (Array.isArray(actual) || Array.isArray(expected)) {
    assert.ok(Array.isArray(actual) && Array.isArray(expected), `${location}: array mismatch`);
    assert.equal(actual.length, expected.length, `${location}: array length mismatch`);
    for (let index = 0; index < actual.length; index += 1) {
      assertSemanticEqual(actual[index], expected[index], `${location}[${index}]`);
    }
    return;
  }
  if (actual !== null && expected !== null && typeof actual === "object" && typeof expected === "object") {
    const actualKeys = Object.keys(actual).sort();
    const expectedKeys = Object.keys(expected).sort();
    assert.deepEqual(actualKeys, expectedKeys, `${location}: object keys differ`);
    for (const key of actualKeys) {
      assertSemanticEqual(actual[key], expected[key], `${location}.${key}`);
    }
    return;
  }
  assert.equal(actual, expected, `${location}: value mismatch`);
}

async function javascriptCorpora(page) {
  return page.evaluate(async () => {
    const noon = await import("/web/noon-authoring.js");
    await noon.initNoon();

    const parity = {};

    {
      const scene = new noon.Scene();
      const circle = new noon.Circle(1.0);
      scene.add(circle);
      scene.play(
        circle.animate().shift(noon.RIGHT).rotate(0.25),
        { runTime: 2.0, rateFunc: noon.smooth },
      );
      parity.animate_options = scene.toJSON();
    }

    {
      const scene = new noon.Scene();
      const circle = new noon.Circle(0.5);
      scene.add(circle);
      scene.play(noon.Create(circle), { runTime: 1.0 });
      scene.play(noon.FadeOut(circle), { runTime: 0.5 });
      scene.play(noon.FadeIn(circle), { runTime: 0.5 });
      parity.lifecycle = scene.toJSON();
    }

    const examples = await import("/web/js/examples/manim-quickstart-equivalents.js");
    const quickstart = {};
    for (const [name, build] of Object.entries(examples.quickstartEquivalents)) {
      quickstart[name] = build().toJSON();
    }

    const galleryExamples = await import("/web/js/examples/manim-gallery-moving-around.js");
    const gallery = {};
    for (const [name, build] of Object.entries(galleryExamples.galleryMovingAround)) {
      gallery[name] = build().toJSON();
    }

    return { parity, quickstart, gallery };
  });
}

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
  throw new Error(`Parity server did not start: ${lastError}\n${serverOutput}`);
}

let browser = null;
try {
  const rust = rustCorpus();
  const rustQuickstart = rustCorpus("manim_quickstart_equivalents");
  const rustMovingAround = rustCorpus("manim_gallery_moving_around");
  assert.deepEqual([...rust.keys()].sort(), Object.keys(pythonCases).sort());
  assert.deepEqual([...rustQuickstart.keys()].sort(), [...quickstartEquivalentCases].sort());
  assert.deepEqual([...rustMovingAround.keys()], ["MovingAround"]);

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

  for (const [name, source] of Object.entries(pythonCases)) {
    const result = await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      source,
    );
    assert.equal(result.kind, "scene_document", `${name}: Python authoring failed`);
    assertSemanticEqual(result.document, rust.get(name), `${name}: python/rust`);
  }

  const movingAroundResult = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    movingAroundPython,
  );
  assert.equal(movingAroundResult.kind, "scene_document", "MovingAround: Python authoring failed");
  assertSemanticEqual(
    movingAroundResult.document,
    rustMovingAround.get("MovingAround"),
    "MovingAround: python/rust",
  );

  const javascript = await javascriptCorpora(page);
  assert.deepEqual(Object.keys(javascript.parity).sort(), [...javascriptParityCases].sort());
  for (const [name, document] of Object.entries(javascript.parity)) {
    assert.ok(rust.has(name), `${name}: missing Rust parity reference`);
    assertSemanticEqual(document, rust.get(name), `${name}: javascript/rust`);
  }

  assert.deepEqual(Object.keys(javascript.quickstart).sort(), [...quickstartEquivalentCases].sort());
  for (const [name, document] of Object.entries(javascript.quickstart)) {
    assert.ok(rustQuickstart.has(name), `${name}: missing Rust Quickstart reference`);
    assertSemanticEqual(document, rustQuickstart.get(name), `${name}: quickstart javascript/rust`);
  }

  assert.deepEqual(Object.keys(javascript.gallery), ["MovingAround"]);
  assertSemanticEqual(
    javascript.gallery.MovingAround,
    rustMovingAround.get("MovingAround"),
    "MovingAround: javascript/rust",
  );

  assert.equal(errors.length, 0, errors.join("\n"));
  console.log(
    "Python/Rust parity, JavaScript parity, Rust/JavaScript Quickstart equivalence, and MovingAround tri-language equivalence passed",
  );
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
