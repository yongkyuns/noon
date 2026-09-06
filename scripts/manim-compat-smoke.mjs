import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4175;
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
  throw new Error(`Manim compatibility smoke server did not start: ${lastError}\n${serverOutput}`);
}

const foundationSource = `
from noon import *

class Demo(Scene):
    def construct(self):
        assert abs(smooth(0.25) - 0.07010372) < 1e-7
        assert abs(smooth(0.5) - 0.5) < 1e-12
        assert abs(smooth(0.75) - 0.92989628) < 1e-7
        assert abs(rush_into(0.5) - 2.0 * smooth(0.25)) < 1e-12
        assert abs(rush_from(0.5) - (2.0 * smooth(0.75) - 1.0)) < 1e-12
        assert abs(there_and_back(0.25) - smooth(0.5)) < 1e-12

        import _manim_compat as _compat_impl
        original_circle_ir = _compat_impl._ir.Circle
        _compat_impl._ir.Circle = lambda *args, **kwargs: (_ for _ in ()).throw(AssertionError("shared Circle constructor must bypass Python IR"))
        try:
            shared_constructed = Circle(radius=0.33)
        finally:
            _compat_impl._ir.Circle = original_circle_ir
        assert abs(shared_constructed.radius - 0.33) < 1e-12

        circle = Circle(radius=0.6, color=BLUE)
        square = Square(side_length=1.0, color=PINK).next_to(circle, RIGHT)
        assert isinstance(circle, Circle)
        assert isinstance(circle, VMobject)
        assert type(circle.copy()) is Circle

        self.play(
            Create(circle),
            Create(square),
            run_time=1.25,
            rate_func=smooth,
        )
        self.play(
            circle.animate.shift((0.0, 1.0, 0.0)),
            run_time=0.75,
            rate_func=linear,
        )
        self.play(FadeIn(Circle(radius=0.2, color=GREEN)), run_time=0.25)
`;

const phaseBSource = `
from noon import *

class GroupAndSceneMembership(Scene):
    def construct(self):
        left = Circle(radius=0.35, color=BLUE)
        right = Square(side_length=0.7, color=PINK)
        pair = VGroup(left, right).arrange(RIGHT, buff=0.4)

        assert int(pair._semantic_family_handle.memberCount) == 2
        pair.add(left)
        assert len(pair) == 2
        assert int(pair._semantic_family_handle.memberCount) == 2
        layout = pair._semantic_family_handle.layoutSession()
        layout.includeMobject(left._semantic_handle)
        layout.includeMobject(right._semantic_handle)
        assert abs(float(layout.width) - pair.width) < 1e-12
        assert abs(float(layout.height) - pair.height) < 1e-12
        alias = VGroup(left)
        assert int(alias._semantic_family_handle.memberCount) == 1

        assert isinstance(pair, Mobject)
        assert isinstance(pair, Group)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        self.play(pair.animate.shift(UP).scale(0.8), run_time=0.5, rate_func=smooth)
        self.remove(pair)
        assert self.mobjects == []

        self.wait(0.1)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        replacement = Circle(radius=0.2, color=GREEN)
        self.replace(pair, replacement)
        assert len(self.mobjects) == 1 and self.mobjects[0] is replacement

        # set_y is intentionally not one of Noon's old fixed animation-builder methods.
        self.play(replacement.animate.set_y(1.5), run_time=0.4, rate_func=linear)
        self.clear()
        assert self.mobjects == []

        intro = VGroup(
            Circle(radius=0.18, color=BLUE),
            Square(side_length=0.36, color=PINK),
        ).arrange(RIGHT, buff=0.2)
        self.play(FadeIn(intro), run_time=0.25)
        assert len(self.mobjects) == 1 and self.mobjects[0] is intro
        self.play(FadeOut(intro), run_time=0.25)
        assert self.mobjects == []
`;

const defaultVmobjectStyleSource = `
from noon import *

class DefaultVmobjectStyle(Scene):
    def construct(self):
        circle = Circle()
        assert abs(circle.get_fill_opacity() - 0.0) < 1e-12
        assert abs(circle.get_stroke_opacity() - 1.0) < 1e-12
        assert abs(circle.style["stroke_width"] - 0.04) < 1e-9
        assert circle.style["stroke_width_mode"] == "screen_space"
        assert circle.style["stroke_join"] == "miter"
        assert circle.style["stroke_cap"] == "butt"
        assert abs(circle.style["fill"]["red"] - RED.red) < 1e-7
        assert abs(circle.style["stroke"]["red"] - RED.red) < 1e-7

        explicit = Square(stroke_width=10)
        assert abs(explicit.style["stroke_width"] - 0.10) < 1e-9
        explicit.set_stroke(width=20)
        assert abs(explicit.style["stroke_width"] - 0.20) < 1e-9

        filled = Circle(fill_color=PINK, fill_opacity=0.5)
        assert abs(filled.get_fill_opacity() - 0.5) < 1e-12
        assert abs(filled.style["stroke_width"] - 0.04) < 1e-9
        self.add(circle, explicit, filled)
`;

const animateParitySource = `
from noon import *

class AnimateParity(Scene):
    def construct(self):
        detached = Circle(radius=0.25, color=BLUE)
        self.play(
            detached.animate(run_time=2.0, rate_func=linear)
                .shift(RIGHT)
                .set_y(1.0)
        )
        assert len(self.mobjects) == 1 and self.mobjects[0] is detached

        square = Square(side_length=0.4, color=PINK)
        self.play(
            square.animate(run_time=2.0).shift(UP),
            detached.animate(run_time=0.5, rate_func=linear).shift(LEFT),
        )

        pair = VGroup(
            Circle(radius=0.15, color=GREEN),
            Square(side_length=0.3, color=RED),
        ).arrange(RIGHT, buff=0.15)
        self.play(pair.animate(run_time=1.2, lag_ratio=0.5).shift(UP))

        override = Circle(radius=0.2, color=PURPLE)
        self.play(
            override.animate(run_time=3.0, rate_func=linear).shift(RIGHT),
            run_time=0.4,
            rate_func=smooth,
        )

        late_args = Circle().animate
        late_args.shift(RIGHT)
        try:
            late_args(run_time=2.0)
            raise AssertionError("animation kwargs after method access must fail")
        except ValueError as error:
            assert "before accessing methods" in str(error)

        duplicate_args = Circle().animate(run_time=1.0)
        try:
            duplicate_args(rate_func=linear)
            raise AssertionError("animation kwargs can only be passed once")
        except ValueError as error:
            assert "only be passed once" in str(error)
`;

const queryTransformSource = `
from noon import *

class SharedQueryTransforms(Scene):
    def construct(self):
        box = Rectangle(width=2.0, height=1.0).shift(RIGHT * 0.7 + UP * 0.3)
        assert abs(box.get_left().x + 0.3) < 1e-9
        assert abs(box.get_right().x - 1.7) < 1e-9
        assert abs(box.get_top().y - 0.8) < 1e-9
        assert abs(box.get_x(LEFT) + 0.3) < 1e-9

        box.set_coord(-1.5, 0, LEFT).set_coord(1.25, 1, UP)
        assert abs(box.get_left().x + 1.5) < 1e-9
        assert abs(box.get_top().y - 1.25) < 1e-9
        box.width = 3.0
        box.stretch_to_fit_height(2.0)
        assert abs(box.width - 3.0) < 1e-9
        assert abs(box.height - 2.0) < 1e-9

        target = Circle(radius=0.4).shift(RIGHT * 1.2 + DOWN * 0.4)
        box.match_x(target).match_y(target)
        assert abs(box.get_x() - target.get_x()) < 1e-9
        assert abs(box.get_y() - target.get_y()) < 1e-9

        orbit = Square(side_length=0.5).shift(RIGHT * 1.5 + UP * 0.5)
        orbit.rotate_about_origin(PI / 2)
        assert abs(orbit.get_x() + 0.5) < 1e-9
        assert abs(orbit.get_y() - 1.5) < 1e-9
        self.add(box, target, orbit)
`;

const uncreateSource = `
from noon import *

class UncreateLifecycle(Scene):
    def construct(self):
        first = Square(side_length=0.6, color=BLUE)
        kept = Circle(radius=0.25, color=PINK)
        forward = Square(side_length=0.4, color=GREEN)
        self.add(first, kept, forward)
        self.play(Uncreate(first), run_time=2.0, rate_func=rush_into)
        assert first not in self.mobjects

        self.play(Uncreate(kept, remover=False), run_time=1.0)
        assert kept in self.mobjects

        self.play(Uncreate(forward, reverse_rate_function=False), run_time=1.0)
        assert forward not in self.mobjects
`;

const rateFunctionSource = `
from noon import *

class SharedRateFunctions(Scene):
    def construct(self):
        circle = Circle(radius=0.2, color=BLUE)
        self.add(circle)
        self.play(circle.animate.shift(RIGHT), run_time=0.2)
        self.play(circle.animate.shift(LEFT), run_time=0.2, rate_func=rush_into)
        self.play(circle.animate.shift(RIGHT), run_time=0.2, rate_func=rush_from)
        self.play(circle.animate.shift(LEFT), run_time=0.2, rate_func=there_and_back)
`;

const concurrentFamilySource = `
from noon import *

class ConcurrentRetainedFamilies(Scene):
    def construct(self):
        short = Text("AB")
        long = Text("ABCDEFGHIJKLMNOPQRST")
        self.play(Write(short), Write(long), rate_func=linear)
`;

const overlappingFamilySource = `
from noon import *

class OverlappingRetainedFamilies(Scene):
    def construct(self):
        text = Text("AB")
        self.play(Write(text), Unwrite(text), rate_func=linear)
`;

const mixedFamilyOrdinarySource = `
from noon import *

class MixedFamilyOrdinary(Scene):
    def construct(self):
        circle = Circle(radius=0.28, color=BLUE).shift(LEFT)
        text = Text("AB")
        self.play(circle.animate.shift(RIGHT), Write(text), rate_func=linear)
`;

const mixedFamilyOrdinaryEditedSource = `
from noon import *

class MixedFamilyOrdinaryEdited(Scene):
    def construct(self):
        text = Text("EDITED")
        square = Square(side_length=0.5, color=PINK).shift(LEFT)
        circle = Circle(radius=0.22, color=GREEN).shift(RIGHT)
        self.play(
            Write(text),
            square.animate.shift(UP),
            circle.animate.shift(LEFT),
            run_time=1.5,
            rate_func=linear,
        )
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

  // #958/#61 prerequisite: every geometry wrapper has store-scoped identity,
  // including independent copies/targets after the JS store wrapper is released.
  const handleOwnership = await page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const store = new wasm.WasmAuthoringStore();
    const otherStore = new wasm.WasmAuthoringStore();
    const circle = store.createManimCircle(0.6);
    const foreign = otherStore.createManimCircle(0.6);
    const family = store.createFamily();
    const copy = circle.cloneHandle();
    const target = circle.targetEditor();
    const identity = (handle) => `${handle.semanticSlot}:${handle.semanticGeneration}`;
    const rejectsForeign = (operation) => {
      try { operation(); } catch (error) {
        return /different authoring stores/.test(String(error));
      }
      return false;
    };
    const sameNumericId = identity(circle) === identity(foreign);
    const foreignAddRejected = rejectsForeign(() => family.addMobject(foreign));
    for (const handle of [circle, copy, target]) family.addMobject(handle);
    const layout = family.layoutSession();
    const foreignLayoutRejected = rejectsForeign(() => layout.includeMobject(foreign));
    for (const handle of [circle, copy, target]) layout.includeMobject(handle);
    const translation = layout.shiftBy(1, 0);
    const foreignTranslationRejected = rejectsForeign(() => translation.applyMobject(foreign));
    store.free();
    otherStore.free();
    // Handles keep the authoritative identity owner alive, independent of JS roots.
    for (const handle of [circle, copy, target]) translation.applyMobject(handle);
    translation.finish();
    const memberCount = family.memberCount;
    for (const handle of [translation, layout, family]) handle.free();
    // Only mobject wrappers now retain the store; copy/target mutation still works.
    copy.shift(2, 0);
    target.shift(-1, 0);
    const result = {
      sameNumericId,
      foreignAddRejected,
      foreignLayoutRejected,
      foreignTranslationRejected,
      identities: [circle, copy, target].map(identity),
      centers: [circle.centerX, copy.centerX, target.centerX],
      memberCount,
    };
    for (const handle of [circle, copy, target, foreign]) handle.free();
    return result;
  });
  assert.equal(handleOwnership.sameNumericId, true, "independent stores may reuse numeric IDs");
  assert.equal(handleOwnership.foreignAddRejected, true);
  assert.equal(handleOwnership.foreignLayoutRejected, true);
  assert.equal(handleOwnership.foreignTranslationRejected, true);
  assert.equal(new Set(handleOwnership.identities).size, 3, "copy/target allocate fresh identities");
  assert.deepEqual(handleOwnership.centers, [1, 3, 0], "copies/targets retain independent state");
  assert.equal(handleOwnership.memberCount, 3, "failed cross-store operations leave membership intact");

  const foundation = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    foundationSource,
  );
  assert.equal(foundation.kind, "scene_document");
  assert.equal(foundation.document.objects.length, 3, "introducer animations should auto-bind objects");

  const foundationProperties = foundation.document.tracks.map((track) => track.property);
  assert.equal(foundationProperties.filter((property) => property === "presence").length, 3);
  assert.equal(foundationProperties.filter((property) => property === "reveal").length, 2);
  assert.ok(foundationProperties.includes("transform"), "animate.shift should lower to transform");

  const revealTracks = foundation.document.tracks.filter((track) => track.property === "reveal");
  assert.ok(
    revealTracks.every((track) => track.timing.easing === "smooth"),
    "rate_func=smooth should lower to the shared smooth semantic ID",
  );
  const transform = foundation.document.tracks.find((track) => track.property === "transform");
  assert.equal(transform.timing.easing, "linear");

  const uncreate = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    uncreateSource,
  );
  assert.equal(uncreate.kind, "scene_document");
  const uncreateReveals = uncreate.document.tracks.filter((track) => track.property === "reveal");
  assert.equal(uncreateReveals.length, 3);
  assert.deepEqual(uncreateReveals[0].values.scalar, { from: 1, to: 0 });
  assert.equal(uncreateReveals[0].timing.easing, "rush_from");
  assert.deepEqual(uncreateReveals[1].values.scalar, { from: 1, to: 0 });
  assert.equal(uncreateReveals[1].timing.easing, "smooth");
  assert.deepEqual(uncreateReveals[2].values.scalar, { from: 0, to: 1 });
  const uncreateRemovals = uncreate.document.tracks.filter(
    (track) => track.property === "presence" && track.values.bool?.from === true && track.values.bool?.to === false,
  );
  assert.equal(uncreateRemovals.length, 2, "remover=False must preserve scene membership");
  assert.equal(uncreateRemovals[0].timing.start_time, 2.0);
  assert.equal(uncreateRemovals[1].timing.start_time, 4.0);

  const phaseB = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    phaseBSource,
  );
  assert.equal(phaseB.kind, "scene_document");
  assert.equal(phaseB.document.objects.length, 5, "groups should lower to flat runtime member objects");
  const phaseBProperties = phaseB.document.tracks.map((track) => track.property);
  assert.equal(
    phaseBProperties.filter((property) => property === "transform").length,
    3,
    "group animate should lower to member transforms and generic set_y should lower once",
  );
  assert.equal(
    phaseBProperties.filter((property) => property === "presence").length,
    12,
    "scene membership and grouped fades should lower to deterministic presence events",
  );

  const defaultVmobjectStyle = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    defaultVmobjectStyleSource,
  );
  assert.equal(defaultVmobjectStyle.kind, "scene_document");
  assert.equal(defaultVmobjectStyle.document.objects.length, 3);
  const defaultStyle = defaultVmobjectStyle.document.objects[0].style;
  assert.equal(defaultStyle.fill.alpha, 0);
  assert.equal(defaultStyle.stroke.alpha, 1);
  assert.ok(Math.abs(defaultStyle.stroke_width - 0.04) < 1e-7);
  assert.equal(defaultStyle.stroke_width_mode, "screen_space");
  assert.equal(defaultStyle.stroke_join, "miter");
  assert.equal(defaultStyle.stroke_cap, "butt");

  const animateParity = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    animateParitySource,
  );
  assert.equal(animateParity.kind, "scene_document");
  assert.equal(animateParity.document.objects.length, 5, "detached animate and groups should auto-bind flat objects");
  const animateTracks = animateParity.document.tracks.filter((track) => track.property === "transform");
  assert.equal(animateTracks.length, 6);

  const byObject = new Map();
  for (const track of animateTracks) {
    const list = byObject.get(track.object) ?? [];
    list.push(track);
    byObject.set(track.object, list);
  }
  assert.equal(byObject.get(0)[0].timing.start_time, 0);
  assert.equal(byObject.get(0)[0].timing.duration, 2);
  assert.equal(byObject.get(0)[0].timing.easing, "linear");
  assert.equal(byObject.get(0)[1].timing.start_time, 2);
  assert.equal(byObject.get(0)[1].timing.duration, 0.5);
  assert.equal(byObject.get(1)[0].timing.start_time, 2);
  assert.equal(byObject.get(1)[0].timing.duration, 2);
  assert.equal(byObject.get(1)[0].timing.easing, "smooth");

  const groupFirst = byObject.get(2)[0];
  const groupSecond = byObject.get(3)[0];
  assert.ok(Math.abs(groupFirst.timing.start_time - 4.0) < 1e-9);
  assert.ok(Math.abs(groupFirst.timing.duration - 0.8) < 1e-9);
  assert.ok(Math.abs(groupSecond.timing.start_time - 4.4) < 1e-9);
  assert.ok(Math.abs(groupSecond.timing.duration - 0.8) < 1e-9);
  assert.equal(groupFirst.timing.easing, "smooth");
  assert.equal(groupSecond.timing.easing, "smooth");

  const overridden = byObject.get(4)[0];
  assert.ok(Math.abs(overridden.timing.start_time - 5.2) < 1e-9);
  assert.ok(Math.abs(overridden.timing.duration - 0.4) < 1e-9);
  assert.equal(
    overridden.timing.easing,
    "smooth",
    "Scene.play kwargs should override builder animation kwargs",
  );

  const queryTransforms = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    queryTransformSource,
  );
  assert.equal(queryTransforms.kind, "scene_document");
  assert.equal(queryTransforms.document.objects.length, 3);

  const sharedRates = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    rateFunctionSource,
  );
  const rateTracks = sharedRates.document.tracks.filter(
    (track) => track.property === "transform",
  );
  assert.deepEqual(
    rateTracks.map((track) => track.timing.easing),
    ["smooth", "rush_into", "rush_from", "there_and_back"],
    "known Python callables should lower directly to shared Rust semantic IDs",
  );

  const concurrent = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    concurrentFamilySource,
  );
  assert.equal(concurrent.kind, "scene_document");
  assert.equal(concurrent.retainedDocument, undefined);
  assert.equal(
    concurrent.sceneSpec.objects.filter((object) => object.content?.kind === "text").length,
    2,
  );
  assert.equal(concurrent.sceneSpec.family_animations.length, 2);
  assert.equal(concurrent.duration, 2, "Scene.play duration must be the maximum child duration");
  const concurrentRequests = concurrent.sceneSpec.family_animations;
  assert.deepEqual(
    concurrentRequests.map((request) => request.spec.start_time),
    [0, 0],
    "concurrent family requests must share the play start time",
  );
  assert.deepEqual(
    concurrentRequests.map((request) => request.spec.duration),
    [1, 2],
    "each Write must retain its independently Rust-derived default duration",
  );
  assert.ok(
    concurrentRequests.every((request) => request.spec.mode === "draw_border_then_fill"),
  );
  const concurrentSerialized = JSON.stringify(concurrentRequests);
  for (const forbidden of ["glyph_id", "atlas_id", "font_bytes"]) {
    assert.equal(concurrentSerialized.includes(forbidden), false, `concurrent requests leaked ${forbidden}`);
  }
  const concurrentRender = await page.evaluate(async (sceneSpec) => {
    const { ExecutionWorkerClient } = await import("./execution-worker-client.js");
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    document.body.appendChild(canvas);
    const errors = [];
    const execution = new ExecutionWorkerClient(canvas, {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });
    async function renderedAt(time) {
      await execution.seek(time);
      let latest = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        latest = await execution.metrics();
        if (errors.length !== 0) throw new Error(errors.join("; "));
        if (
          Math.abs(Number(latest.metrics.time) - time) <= 1e-6 &&
          latest.metrics.objectCount === 2 &&
          latest.metrics.presentedFrames >= 1
        ) {
          return latest;
        }
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error(`concurrent family render did not converge at t=${time}: ${JSON.stringify(latest)}`);
    }
    try {
      await execution.startRetainedCanonical(JSON.stringify(sceneSpec), {
        loopDurationSeconds: 2,
        transportMode: "transferable",
      });
      await execution.pause();
      const first = await renderedAt(0.5);
      const presented = first.metrics.presentedFrames;
      const second = await renderedAt(1.5);
      if (!first.engineMetrics.canonical || !second.engineMetrics.canonical) {
        throw new Error("concurrent family render bypassed canonical retained execution");
      }
      if (second.metrics.presentedFrames <= presented) {
        throw new Error("second concurrent-family seek did not present a new frame");
      }
      return {
        firstTime: first.metrics.time,
        secondTime: second.metrics.time,
        objectCount: second.metrics.objectCount,
        presentedFrames: second.metrics.presentedFrames,
      };
    } finally {
      execution.terminate();
      canvas.remove();
    }
  }, concurrent.sceneSpec);
  assert.equal(concurrentRender.firstTime, 0.5);
  assert.equal(concurrentRender.secondTime, 1.5);
  assert.equal(concurrentRender.objectCount, 2);

  const mixedFirst = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    mixedFamilyOrdinarySource,
  );
  assert.equal(mixedFirst.kind, "scene_document");
  assert.equal(mixedFirst.document.objects.length, 1);
  assert.equal(mixedFirst.retainedDocument, undefined);
  assert.equal(
    mixedFirst.sceneSpec.objects.filter((object) => object.content?.kind === "text").length,
    1,
  );
  assert.equal(mixedFirst.sceneSpec.objects.length, 2);
  assert.equal(mixedFirst.sceneSpec.family_animations.length, 1);
  assert.equal(
    mixedFirst.document.tracks.filter((track) => track.property === "transform").length,
    1,
  );
  assert.equal(mixedFirst.duration, 1);
  assert.equal(mixedFirst.sceneSpec.family_animations[0].spec.start_time, 0);
  assert.equal(mixedFirst.sceneSpec.family_animations[0].spec.duration, 1);

  // Reuse the same Pyodide authoring worker for a source edit. The second Scene must
  // own a fresh Rust canonical authoring context rather than accumulating the first.
  const mixedEdited = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    mixedFamilyOrdinaryEditedSource,
  );
  assert.equal(mixedEdited.kind, "scene_document");
  assert.equal(mixedEdited.document.objects.length, 2);
  assert.equal(mixedEdited.retainedDocument, undefined);
  assert.equal(
    mixedEdited.sceneSpec.objects.filter((object) => object.content?.kind === "text").length,
    1,
  );
  assert.equal(mixedEdited.sceneSpec.objects.length, 3);
  assert.equal(mixedEdited.sceneSpec.family_animations.length, 1, "edited rerun must replace family requests");
  assert.equal(
    mixedEdited.document.tracks.filter((track) => track.property === "transform").length,
    2,
  );
  assert.equal(mixedEdited.duration, 1.5);
  assert.equal(mixedEdited.sceneSpec.family_animations[0].spec.start_time, 0);
  assert.equal(mixedEdited.sceneSpec.family_animations[0].spec.duration, 1.5);

  const mixedRerunRender = await page.evaluate(async ({ first, edited }) => {
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const canvas = document.createElement("canvas");
    canvas.width = 640;
    canvas.height = 360;
    document.body.appendChild(canvas);
    const errors = [];
    const execution = new AuthoringExecutionClient(canvas, {
      onError(error, owner) {
        errors.push(`${owner}: ${error}`);
      },
    });

    async function renderedAt(time, expectedObjectCount) {
      await execution.seek(time);
      let latest = null;
      for (let attempt = 0; attempt < 100; attempt += 1) {
        latest = await execution.metrics();
        if (errors.length !== 0) throw new Error(errors.join("; "));
        if (
          Math.abs(Number(latest.metrics.time) - time) <= 1e-6 &&
          latest.metrics.objectCount === expectedObjectCount &&
          latest.metrics.presentedFrames >= 1
        ) {
          return latest;
        }
        await new Promise((resolve) => setTimeout(resolve, 20));
      }
      throw new Error(
        `mixed edit/rerun render did not converge at t=${time}: ${JSON.stringify(latest)}`,
      );
    }

    try {
      await execution.startRetainedCanonical(JSON.stringify(first.sceneSpec), {
        loopDurationSeconds: first.duration,
        transportMode: "transferable",
      });
      await execution.pause();
      const firstReport = await renderedAt(0.5, 2);
      if (!firstReport.engineMetrics.canonical) {
        throw new Error("first mixed scene bypassed canonical retained execution");
      }

      const reconcile = await execution.reconcileScene(JSON.stringify(edited.document), {
        sceneSpecJson: JSON.stringify(edited.sceneSpec),
        loopDurationSeconds: edited.duration,
      });
      if (!reconcile.rebuilt || reconcile.mode !== "retained") {
        throw new Error(`edited mixed scene did not rebuild retained execution: ${JSON.stringify(reconcile)}`);
      }
      if (execution.canvas !== canvas) {
        throw new Error("retained edit/rerun replaced the persistent authoring canvas");
      }

      await execution.pause();
      const editedReport = await renderedAt(0.75, 3);
      if (!editedReport.engineMetrics.canonical) {
        throw new Error("edited mixed scene bypassed canonical retained execution");
      }
      return {
        firstObjectCount: firstReport.metrics.objectCount,
        editedObjectCount: editedReport.metrics.objectCount,
        firstTime: firstReport.metrics.time,
        editedTime: editedReport.metrics.time,
        rebuilt: reconcile.rebuilt,
        mode: reconcile.mode,
      };
    } finally {
      execution.terminate();
      canvas.remove();
    }
  }, { first: mixedFirst, edited: mixedEdited });
  assert.deepEqual(mixedRerunRender, {
    firstObjectCount: 2,
    editedObjectCount: 3,
    firstTime: 0.5,
    editedTime: 0.75,
    rebuilt: true,
    mode: "retained",
  });

  let overlapError = null;
  try {
    await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      overlappingFamilySource,
    );
  } catch (error) {
    overlapError = String(error);
  }
  assert.match(
    overlapError ?? "",
    /disjoint family leaves/,
    "same-leaf concurrent family ownership must fail before lifecycle mutation",
  );

  let zError = null;
  try {
    await page.evaluate(
      (pythonSource) => window.noonManimCompat.run(pythonSource),
      `from noon import *\nresult = Scene()\nLine((0, 0, 1), (1, 0, 0))`,
    );
  } catch (error) {
    zError = String(error);
  }
  assert.match(zError ?? "", /z must be 0/, "non-zero z should fail explicitly");

  assert.deepEqual(errors, [], `browser errors while testing Manim compatibility:\n${errors.join("\n")}`);
  console.log(
    "Manim compatibility smoke passed: construct discovery, shape classes, scene/group semantics, callable and chained animate builders, detached animate auto-add, per-animation timing, play overrides, concurrent retained-family play, mixed family/ordinary play with retained edit-rerun rebuild, shared detached query/dimension transforms, z=0 vectors, and shared deterministic Manim rate-function lowering.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
