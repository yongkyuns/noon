import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4198;
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
  throw new Error(`mixed retained family smoke server did not start: ${lastError}\n${serverOutput}`);
}

const propertyFirstSource = `
from noon import *

class MixedRetainedPropertyFirst(Scene):
    def construct(self):
        moving = Text("MOVE")
        writing = Text("WRITE")
        self.play(
            moving.animate.shift(RIGHT),
            Write(writing),
            run_time=1.0,
            rate_func=linear,
        )
`;

const editedFamilyFirstSource = `
from noon import *

class MixedRetainedFamilyFirst(Scene):
    def construct(self):
        writing = Text("EDITED")
        moving = Text("SHIFT")
        appearing = Text("FADE")
        self.play(
            Write(writing),
            moving.animate.shift(UP),
            FadeIn(appearing),
            run_time=1.0,
            rate_func=linear,
        )
`;

const sameLeafSource = `
from noon import *

class MixedRetainedSameLeaf(Scene):
    def construct(self):
        label = Text("ONE")
        try:
            self.play(
                Write(label),
                label.animate.shift(RIGHT),
                run_time=0.25,
            )
            raise AssertionError("same-leaf family/property ownership must fail")
        except ValueError as error:
            assert "disjoint scene leaves" in str(error)
        assert label._scene is None
        assert label._object is None
        assert label._retained_object_id is None
        self.play(Write(label), run_time=0.25, rate_func=linear)
`;

const rollbackSource = `
from noon import *

class MixedRetainedRollback(Scene):
    def construct(self):
        writing = Text("ROLLBACK")
        moving = Text("MOVE")
        try:
            self.play(
                Write(writing),
                moving.animate(run_time=-1.0).shift(RIGHT),
            )
            raise AssertionError("negative retained run_time must fail")
        except ValueError:
            pass
        assert writing._scene is None
        assert writing._object is None
        assert writing._retained_object_id is None
        assert moving._scene is None
        assert moving._object is None
        assert moving._retained_object_id is None
        self.play(
            Write(writing),
            moving.animate.shift(RIGHT),
            run_time=0.5,
            rate_func=linear,
        )
`;

function retainedSources(result) {
  return (result.retainedDocument?.objects ?? []).map((object) => object.text.source);
}

function tracksFor(result, property) {
  return (result.retainedDocument?.tracks ?? []).filter((track) => track.property === property);
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

  // Both source versions run through the same Pyodide authoring worker. Object order
  // must follow source binding order even though both objects use retained execution.
  const propertyFirst = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    propertyFirstSource,
  );
  assert.equal(propertyFirst.kind, "scene_document");
  assert.equal(propertyFirst.document.objects.length, 0);
  assert.deepEqual(retainedSources(propertyFirst), ["MOVE", "WRITE"]);
  assert.equal(propertyFirst.sceneSpec.objects.length, 2);
  assert.equal(propertyFirst.sceneSpec.family_animations.length, 1);
  assert.equal(propertyFirst.retainedDocument.family_animations.length, 1);
  assert.equal(propertyFirst.duration, 1);
  const firstPositions = tracksFor(propertyFirst, "position");
  assert.equal(firstPositions.length, 1);
  assert.equal(firstPositions[0].object, 0, "property-first Text must own the first retained object slot");
  assert.deepEqual(firstPositions[0].values.vec2, {
    from: { x: 0, y: 0 },
    to: { x: 1, y: 0 },
  });
  assert.equal(firstPositions[0].timing.start_time, 0);
  assert.equal(firstPositions[0].timing.duration, 1);
  assert.equal(firstPositions[0].timing.easing, "linear");

  const edited = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    editedFamilyFirstSource,
  );
  assert.equal(edited.kind, "scene_document");
  assert.equal(edited.document.objects.length, 0);
  assert.deepEqual(retainedSources(edited), ["EDITED", "SHIFT", "FADE"]);
  assert.equal(edited.sceneSpec.objects.length, 3);
  assert.equal(edited.sceneSpec.family_animations.length, 1);
  assert.equal(edited.retainedDocument.family_animations.length, 1);
  assert.equal(edited.duration, 1);
  const editedPositions = tracksFor(edited, "position");
  assert.equal(editedPositions.length, 1);
  assert.equal(editedPositions[0].object, 1, "family-first source order must remain canonical painter order");
  assert.deepEqual(editedPositions[0].values.vec2, {
    from: { x: 0, y: 0 },
    to: { x: 0, y: 1 },
  });
  const editedPresence = tracksFor(edited, "presence");
  const editedAppearance = tracksFor(edited, "appearance");
  assert.ok(
    editedPresence.some((track) => track.object === 2 && track.values.bool?.from === false && track.values.bool?.to === true),
    "mixed retained FadeIn must keep retained lifecycle ownership",
  );
  assert.ok(
    editedAppearance.some((track) => track.object === 2 && track.values.scalar?.from === 0 && track.values.scalar?.to === 1),
    "mixed retained FadeIn must keep retained appearance-track ownership",
  );

  const sameLeaf = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    sameLeafSource,
  );
  assert.deepEqual(retainedSources(sameLeaf), ["ONE"]);
  assert.equal(sameLeaf.sceneSpec.objects.length, 1);
  assert.equal(sameLeaf.sceneSpec.family_animations.length, 1);
  assert.equal(tracksFor(sameLeaf, "position").length, 0, "rejected same-leaf play must not leak a retained track");

  const rollback = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    rollbackSource,
  );
  assert.deepEqual(retainedSources(rollback), ["ROLLBACK", "MOVE"]);
  assert.equal(rollback.sceneSpec.objects.length, 2);
  assert.equal(rollback.sceneSpec.family_animations.length, 1, "failed family request must be rolled back before retry");
  assert.equal(tracksFor(rollback, "position").length, 1, "failed retained property track must not survive retry");

  // Edit -> rerun uses the same browser execution owner. Rebuilding from A to B must
  // replace the canonical object/request set rather than accumulate the first run.
  const rebuild = await page.evaluate(async ({ first, second }) => {
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    document.body.appendChild(canvas);
    const errors = [];
    const execution = new AuthoringExecutionClient(canvas, {
      onError(error) {
        errors.push(String(error));
      },
    });

    async function waitForObjectCount(expected) {
      let latest = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        latest = await execution.metrics();
        if (errors.length !== 0) throw new Error(errors.join("; "));
        if (latest.metrics?.objectCount === expected) return latest;
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error(`retained object count did not converge to ${expected}: ${JSON.stringify(latest)}`);
    }

    try {
      await execution.startRetainedCanonical(JSON.stringify(first.sceneSpec), {
        loopDurationSeconds: first.duration,
        transportMode: "transferable",
      });
      const persistentCanvas = execution.canvas;
      const before = await waitForObjectCount(2);
      const reconciled = await execution.reconcileScene(JSON.stringify(second.document), {
        sceneSpecJson: JSON.stringify(second.sceneSpec),
        loopDurationSeconds: second.duration,
      });
      const after = await waitForObjectCount(3);
      return {
        beforeCount: before.metrics.objectCount,
        afterCount: after.metrics.objectCount,
        beforeCanonical: Boolean(before.engineMetrics?.canonical),
        afterCanonical: Boolean(after.engineMetrics?.canonical),
        rebuilt: reconciled.rebuilt,
        mode: reconciled.mode,
        sameCanvas: execution.canvas === persistentCanvas,
      };
    } finally {
      execution.terminate();
      execution.canvas?.remove();
    }
  }, { first: propertyFirst, second: edited });

  assert.deepEqual(rebuild, {
    beforeCount: 2,
    afterCount: 3,
    beforeCanonical: true,
    afterCanonical: true,
    rebuilt: true,
    mode: "retained",
    sameCanvas: true,
  });

  assert.deepEqual(errors, [], `browser errors while testing mixed retained family composition:\n${errors.join("\n")}`);
  console.log("Mixed retained family/property animation smoke passed, including edit -> rerun rebuild.");
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
