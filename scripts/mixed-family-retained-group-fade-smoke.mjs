import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PNG } from "pngjs";
import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4199;
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
  throw new Error(`mixed retained group fade smoke server did not start: ${lastError}\n${serverOutput}`);
}

const writeFirstSource = `
from noon import *

class MixedRetainedGroupFadeWriteFirst(Scene):
    def construct(self):
        writing = Text("WRITE")
        writing.shift(UP)
        first = Text("A")
        second = Text("B")
        labels = VGroup(first, second).arrange(RIGHT).shift(DOWN)
        self.play(
            Write(writing),
            FadeIn(labels),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [writing, labels]
`;

const fadeFirstEditedSource = `
from noon import *

class MixedRetainedGroupFadeFirst(Scene):
    def construct(self):
        first = Text("A2")
        second = Text("B2")
        third = Text("C2")
        labels = VGroup(first, second, third)
        writing = Text("EDITED")
        self.play(
            FadeIn(labels),
            Write(writing),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [labels, writing]
`;

const sameLeafSource = `
from noon import *

class MixedRetainedGroupFadeSameLeaf(Scene):
    def construct(self):
        shared = Text("SHARED")
        peer = Text("PEER")
        labels = VGroup(shared, peer)
        try:
            self.play(
                Write(shared),
                FadeIn(labels),
                run_time=0.25,
            )
            raise AssertionError("same-leaf family/group-fade ownership must fail")
        except ValueError as error:
            assert "disjoint scene leaves" in str(error)
        assert self.mobjects == []
        self.play(FadeIn(labels), run_time=0.25, rate_func=linear)
        self.play(FadeOut(labels), run_time=0.25, rate_func=linear)
`;

const rollbackSource = `
from noon import *

class MixedRetainedGroupFadeRollback(Scene):
    def construct(self):
        first = Text("A")
        second = Text("B")
        labels = VGroup(first, second)
        writing = Text("WRITE")
        moving = Text("MOVE")
        try:
            self.play(
                FadeIn(labels),
                Write(writing),
                moving.animate(run_time=-1.0).shift(RIGHT),
            )
            raise AssertionError("negative sibling run_time must fail")
        except ValueError:
            pass
        assert self.mobjects == []
        assert abs(moving.get_center()[0]) < 1e-6
        self.play(
            Write(writing),
            FadeIn(labels),
            moving.animate.shift(RIGHT),
            run_time=0.5,
            rate_func=linear,
        )
`;

const laggedSource = `
from noon import *

class MixedRetainedGroupFadeLagged(Scene):
    def construct(self):
        labels = VGroup(Text("A"), Text("B"))
        writing = Text("WRITE")
        self.play(
            Write(writing),
            FadeIn(labels, lag_ratio=0.25),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [writing, labels]
`;

function visibleTextRows(buffer) {
  const png = PNG.sync.read(buffer);
  const result = { total: 0, upper: 0, lower: 0, upperBrightness: 0, lowerBrightness: 0 };
  for (let offset = 0; offset < png.data.length; offset += 4) {
    const red = png.data[offset];
    const green = png.data[offset + 1];
    const blue = png.data[offset + 2];
    const brightness = Math.max(red, green, blue);
    if (brightness < 24) continue;
    result.total += 1;
    const y = Math.floor(offset / 4 / png.width);
    if (y < png.height / 2) {
      result.upper += 1;
      result.upperBrightness += brightness;
    } else {
      result.lower += 1;
      result.lowerBrightness += brightness;
    }
  }
  return result;
}

async function startSampledSource(page, source) {
  await page.evaluate(async (pythonSource) => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    const canvas = document.createElement("canvas");
    canvas.id = "mixed-family-fade-runtime";
    canvas.width = 640;
    canvas.height = 360;
    document.body.append(canvas);
    let resolveAttached;
    let rejectAttached;
    const attached = new Promise((resolve, reject) => {
      resolveAttached = resolve;
      rejectAttached = reject;
    });
    const runtimeErrors = [];
    const execution = new AuthoringExecutionClient(canvas, {
      onError(error, owner) {
        const failure = new Error(`${owner}: ${error}`);
        runtimeErrors.push(failure.message);
        rejectAttached(failure);
      },
    });
    const authored = authoring.run(pythonSource, {}, {
      async onSemanticContinuation(registration) {
        await execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: authoring,
          loopDurationSeconds: registration.duration,
          transportMode: "transferable",
          pacing: "external_samples",
        });
        resolveAttached();
      },
    });
    authored.catch(rejectAttached);
    await attached;
    window.mixedFamilyFadeProof = { authoring, execution, authored, canvas, runtimeErrors };
  }, source);
}

async function stopSampledSource(page) {
  await page.evaluate(() => {
    const proof = window.mixedFamilyFadeProof;
    proof.execution.terminate();
    proof.authoring.terminate();
    proof.canvas.remove();
    window.mixedFamilyFadeProof = null;
  });
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

  // Reconcile every public source through one shared execution owner. This
  // exercises both argument orders, atomic rejection, positive lag scheduling,
  // and replacement of the previous source without replacing the canvas.
  const observed = await page.evaluate(
    (sources) => window.noonManimCompat.runLiveSources(sources),
    [writeFirstSource, fadeFirstEditedSource, sameLeafSource, rollbackSource, laggedSource],
  );
  assert.equal(observed.sameCanvas, true, "shared source reruns must preserve the canvas");
  assert.deepEqual(observed.results.map((result) => result.duration), [1, 1, 0.5, 0.5, 1]);
  assert.deepEqual(observed.results.map((result) => result.metrics.objectCount), [3, 4, 0, 4, 3]);
  for (const result of observed.results) {
    assert.ok(result.metrics.presentedFrames > 0, "shared Text family Fade must present");
  }

  // Sample the same shared semantic execution used by normal live authoring.
  // Write occupies the upper half and the family FadeIn the lower half, so one
  // midpoint proves both planned effects reach the renderer concurrently.
  await startSampledSource(page, writeFirstSource);
  try {
    const canvas = page.locator("#mixed-family-fade-runtime");
    await page.evaluate(() => window.mixedFamilyFadeProof.execution.sampleToAuthoredTime(0));
    const initial = visibleTextRows(await canvas.screenshot());
    await page.evaluate(() => window.mixedFamilyFadeProof.execution.sampleToAuthoredTime(0.5));
    const midpoint = visibleTextRows(await canvas.screenshot());
    const completed = await page.evaluate(async () => {
      const proof = window.mixedFamilyFadeProof;
      const [, authored] = await Promise.all([
        proof.execution.sampleToAuthoredTime(1),
        proof.authored,
      ]);
      return {
        duration: authored.duration,
        metrics: (await proof.execution.metrics()).metrics,
        sameCanvas: proof.execution.canvas === proof.canvas,
        errors: proof.runtimeErrors,
      };
    });
    const endpoint = visibleTextRows(await canvas.screenshot());
    assert.equal(initial.total, 0, `detached FadeIn/Write must start hidden: ${JSON.stringify(initial)}`);
    assert.ok(midpoint.upper > 0, `Text Write did not draw at midpoint: ${JSON.stringify(midpoint)}`);
    assert.ok(midpoint.lower > 0, `Text family FadeIn did not draw at midpoint: ${JSON.stringify(midpoint)}`);
    assert.ok(endpoint.upper > 0 && endpoint.lower > 0,
      `Text Write/FadeIn endpoints must remain visible: ${JSON.stringify(endpoint)}`);
    assert.ok(midpoint.lowerBrightness < endpoint.lowerBrightness * 0.9,
      `Text family FadeIn must remain visibly dimmer at midpoint: ${JSON.stringify({ midpoint, endpoint })}`);
    assert.equal(completed.duration, 1);
    assert.equal(completed.metrics.objectCount, 3);
    assert.ok(completed.metrics.presentedFrames > 0);
    assert.equal(completed.sameCanvas, true);
    assert.deepEqual(completed.errors, []);
  } finally {
    await stopSampledSource(page);
  }
  assert.deepEqual(
    errors,
    [],
    `browser errors while testing shared Text family fades:\n${errors.join("\n")}`,
  );
  console.log("Shared Text family fade composition passed, including overlap rejection, rollback, lag, pixels, and source reuse.");
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
