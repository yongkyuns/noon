import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
        first = Text("A")
        second = Text("B")
        labels = VGroup(first, second)
        self.play(
            Write(writing),
            FadeIn(labels),
            run_time=1.0,
            rate_func=linear,
        )
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
        assert shared._scene is None
        assert shared._object is None
        assert shared._retained_object_id is None
        assert peer._scene is None
        assert peer._object is None
        assert peer._retained_object_id is None
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
        for member in (first, second, writing, moving):
            assert member._scene is None
            assert member._object is None
            assert member._retained_object_id is None
        self.play(
            Write(writing),
            FadeIn(labels),
            moving.animate.shift(RIGHT),
            run_time=0.5,
            rate_func=linear,
        )
`;

const lagFailureSource = `
from noon import *

class MixedRetainedGroupFadeLagFailure(Scene):
    def construct(self):
        labels = VGroup(Text("A"), Text("B"))
        writing = Text("WRITE")
        try:
            self.play(
                Write(writing),
                FadeIn(labels),
                lag_ratio=0.25,
            )
            raise AssertionError("group fade play lag_ratio must fail")
        except NotImplementedError as error:
            assert "shared retained family scheduling" in str(error)
        for member in (*labels.submobjects, writing):
            assert member._scene is None
            assert member._object is None
            assert member._retained_object_id is None
`;

function retainedSources(result) {
  return (result.retainedDocument?.objects ?? []).map((object) => object.text.source);
}

function tracksFor(result, property) {
  return (result.retainedDocument?.tracks ?? []).filter((track) => track.property === property);
}

function assertFadeTracks(result, objectIndexes) {
  const presence = tracksFor(result, "presence");
  const appearance = tracksFor(result, "appearance");
  for (const object of objectIndexes) {
    assert.ok(
      presence.some(
        (track) =>
          track.object === object &&
          track.values.bool?.from === false &&
          track.values.bool?.to === true,
      ),
      `missing FadeIn presence track for retained object ${object}`,
    );
    assert.ok(
      appearance.some(
        (track) =>
          track.object === object &&
          track.values.scalar?.from === 0 &&
          track.values.scalar?.to === 1,
      ),
      `missing FadeIn appearance track for retained object ${object}`,
    );
  }
}

function assertFadeOutTracks(result, objectIndexes) {
  const presence = tracksFor(result, "presence");
  const appearance = tracksFor(result, "appearance");
  for (const object of objectIndexes) {
    assert.ok(
      presence.some(
        (track) =>
          track.object === object &&
          track.values.bool?.from === true &&
          track.values.bool?.to === false,
      ),
      `missing FadeOut presence track for retained object ${object}`,
    );
    assert.ok(
      appearance.some(
        (track) =>
          track.object === object &&
          track.values.scalar?.from === 1 &&
          track.values.scalar?.to === 0,
      ),
      `missing FadeOut appearance track for retained object ${object}`,
    );
  }
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

  const writeFirst = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    writeFirstSource,
  );
  assert.equal(writeFirst.kind, "scene_document");
  assert.deepEqual(retainedSources(writeFirst), ["WRITE", "A", "B"]);
  assert.equal(writeFirst.sceneSpec.objects.length, 3);
  assert.equal(writeFirst.sceneSpec.family_animations.length, 1);
  assert.equal(writeFirst.retainedDocument.family_animations.length, 1);
  assert.equal(writeFirst.duration, 1);
  assertFadeTracks(writeFirst, [1, 2]);

  const fadeFirst = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    fadeFirstEditedSource,
  );
  assert.equal(fadeFirst.kind, "scene_document");
  assert.deepEqual(retainedSources(fadeFirst), ["A2", "B2", "C2", "EDITED"]);
  assert.equal(fadeFirst.sceneSpec.objects.length, 4);
  assert.equal(fadeFirst.sceneSpec.family_animations.length, 1);
  assert.equal(fadeFirst.retainedDocument.family_animations.length, 1);
  assertFadeTracks(fadeFirst, [0, 1, 2]);

  const sameLeaf = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    sameLeafSource,
  );
  assert.deepEqual(retainedSources(sameLeaf), ["SHARED", "PEER"]);
  assert.equal((sameLeaf.sceneSpec.family_animations ?? []).length, 0);
  assertFadeTracks(sameLeaf, [0, 1]);
  assertFadeOutTracks(sameLeaf, [0, 1]);

  const rollback = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    rollbackSource,
  );
  assert.deepEqual(retainedSources(rollback), ["WRITE", "A", "B", "MOVE"]);
  assert.equal(rollback.sceneSpec.family_animations.length, 1);
  assert.equal(tracksFor(rollback, "position").length, 1);
  assertFadeTracks(rollback, [1, 2]);

  const lagFailure = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    lagFailureSource,
  );
  assert.equal(lagFailure.sceneSpec.objects.length, 0);
  assert.equal((lagFailure.sceneSpec.family_animations ?? []).length, 0);

  // Reconcile the two source versions through one persistent execution owner. The
  // retained object set must replace 3 -> 4 rather than accumulate the first run.
  const rebuild = await page.evaluate(async ({ first, second }) => {
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    document.body.appendChild(canvas);
    const runtimeErrors = [];
    const execution = new AuthoringExecutionClient(canvas, {
      onError(error) {
        runtimeErrors.push(String(error));
      },
    });

    async function waitForObjectCount(expected) {
      let latest = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        latest = await execution.metrics();
        if (runtimeErrors.length !== 0) throw new Error(runtimeErrors.join("; "));
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
      const before = await waitForObjectCount(3);
      const reconciled = await execution.reconcileScene(JSON.stringify(second.document), {
        sceneSpecJson: JSON.stringify(second.sceneSpec),
        loopDurationSeconds: second.duration,
      });
      const after = await waitForObjectCount(4);
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
  }, { first: writeFirst, second: fadeFirst });

  assert.deepEqual(rebuild, {
    beforeCount: 3,
    afterCount: 4,
    beforeCanonical: true,
    afterCanonical: true,
    rebuilt: true,
    mode: "retained",
    sameCanvas: true,
  });

  assert.deepEqual(
    errors,
    [],
    `browser errors while testing retained family fade batches:\n${errors.join("\n")}`,
  );
  console.log("Mixed retained family Group/VGroup fade batch smoke passed, including standalone FadeOut and edit -> rerun rebuild.");
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}