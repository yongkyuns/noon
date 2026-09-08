import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const uncreateSource = await readFile(
  path.join(repoRoot, "web/python/examples/ordinary_uncreate_options.py"),
  "utf8",
);
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
        self.play(circle.animate.set_y(1.5), run_time=0.4, rate_func=linear)
        self.play(FadeIn(Circle(radius=0.2, color=GREEN)), run_time=0.25)

`;

// Ordinary groups use the same shared family lifecycle as Text families.
const groupFadeSource = `
from noon import *

class GroupFadeLive(Scene):
    def construct(self):
        intro = VGroup(
            Circle(radius=0.18, color=BLUE),
            Square(side_length=0.36, color=PINK),
        ).arrange(RIGHT, buff=0.2)
        self.play(FadeIn(intro), run_time=0.25)
        self.play(FadeOut(intro), run_time=0.25)
`;

const phaseBSource = `
from noon import *

class GroupMembershipLive(Scene):
    def construct(self):
        left = Circle(radius=0.35, color=BLUE)
        right = Square(side_length=0.7, color=PINK)
        pair = VGroup(left, right).arrange(RIGHT, buff=0.4)

        assert int(pair._semantic_family_handle.memberCount) == 2
        pair.add(left)
        assert len(pair) == 2
        assert int(pair._semantic_family_handle.memberCount) == 2
        layout = pair._semantic_family_handle.layout()
        assert abs(float(layout.width) - pair.width) < 1e-12
        assert abs(float(layout.height) - pair.height) < 1e-12
        alias = VGroup(left)
        assert int(alias._semantic_family_handle.memberCount) == 1

        duplicate = VGroup(left, alias, left)
        assert list(duplicate) == [left, alias]
        spare = Circle(radius=0.1)
        cycle = VGroup(pair)
        before = list(pair.submobjects)
        try:
            pair.add(spare, cycle)
            raise AssertionError("authored cyclic batch must fail")
        except Exception as error:
            assert "cycle" in str(error).lower()
        assert pair.submobjects == before
        assert int(pair._semantic_family_handle.memberCount) == 2

        assert isinstance(pair, Mobject)
        assert isinstance(pair, Group)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        # Family transforms are qualified by the live AnimateParity case below.
        self.wait(0.5)
        self.remove(pair)
        assert self.mobjects == []

        self.wait(0.1)
        self.add(pair)
        assert len(self.mobjects) == 1 and self.mobjects[0] is pair

        replacement = Circle(radius=0.2, color=GREEN)
        self.replace(pair, replacement)
        assert len(self.mobjects) == 1 and self.mobjects[0] is replacement

        self.wait(0.4)
        self.clear()
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
        assert abs(detached.get_center()[0] - 1) < 1e-6
        assert abs(detached.get_center()[1] - 1) < 1e-6

        square = Square(side_length=0.4, color=PINK)
        self.play(
            square.animate(run_time=2.0).shift(UP),
            detached.animate(run_time=0.5, rate_func=linear).shift(LEFT),
        )

        pair = VGroup(
            Circle(radius=0.15, color=GREEN),
            Square(side_length=0.3, color=RED),
        ).arrange(RIGHT, buff=0.15)
        spare = Circle(radius=0.1)
        cycle = VGroup(pair)
        before_members = list(pair.submobjects)
        try:
            pair.add(spare, cycle)
        except Exception:
            pass
        else:
            raise AssertionError("cyclic live family batch must fail")
        assert pair.submobjects == before_members
        for member in pair:
            self.live_execution().add(member)
        self.play(pair.animate(run_time=1.2, lag_ratio=0.5).shift(UP))

        override = Circle(radius=0.2, color=PURPLE)
        self.play(
            override.animate(run_time=3.0, rate_func=linear).shift(RIGHT),
            run_time=0.4,
            rate_func=smooth,
        )

        assert abs(override.get_center()[0] - 1) < 1e-6
        assert abs(detached.get_center()[0]) < 1e-6
        assert abs(square.get_center()[1] - 1) < 1e-6
        assert abs(pair.get_center()[1] - 1) < 1e-6

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
        assert short in self.mobjects
        assert long in self.mobjects
`;

const plainTextLifecycleSource = `
from noon import *

class PlainTextLifecycle(Scene):
    def construct(self):
        text = Text("AB")
        self.add(text)
        self.play(Write(text), run_time=1.0, rate_func=linear)
        assert text in self.mobjects
        self.play(Unwrite(text), run_time=1.0, rate_func=linear)
        assert text not in self.mobjects
`;

const overlappingFamilySource = `
from noon import *

class OverlappingRetainedFamilies(Scene):
    def construct(self):
        text = Text("AB")
        self.add(text)
        self.play(Write(text), Unwrite(text), rate_func=linear)
`;

const mixedFamilyOrdinarySource = `
from noon import *

class MixedFamilyOrdinary(Scene):
    def construct(self):
        circle = Circle(radius=0.28, color=BLUE).shift(LEFT)
        text = Text("AB")
        self.play(circle.animate.shift(RIGHT), Write(text), rate_func=linear)
        assert circle in self.mobjects
        assert text in self.mobjects
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
        assert text in self.mobjects
        assert square in self.mobjects
        assert circle in self.mobjects
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

  const sceneOnlyAuthoring = await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const client = new PythonAuthoringClient();
    try {
      await client.ready();
      let patchError = null;
      try {
        await client.run("from _noon_ir import PatchBatch\nresult = PatchBatch(0)");
      } catch (error) {
        patchError = String(error);
      }
      const result = await client.run(
        "import noon\nassert not hasattr(noon, 'PatchBatch')\nresult = noon.Scene()",
      );
      return { patchError, sharedScene: Boolean(result.semanticExecution), terminated: client.terminated };
    } finally {
      client.terminate();
    }
  });
  assert.match(sceneOnlyAuthoring.patchError, /Python authoring result must be a noon.Scene/);
  assert.equal(sceneOnlyAuthoring.sharedScene, true, "worker accepts a shared Scene after an invalid result");
  assert.equal(sceneOnlyAuthoring.terminated, false, "ordinary authoring errors keep the worker reusable");

  // #958/#61 prerequisite: every geometry wrapper has store-scoped identity,
  // including independent copies/targets after the JS store wrapper is released.
  const handleOwnership = await page.evaluate(async () => {
    const wasm = await import("./pkg/noon_web.js");
    await wasm.default();
    const store = new wasm.WasmAuthoringStore();
    const otherStore = new wasm.WasmAuthoringStore();
    const circle = store.createManimCircle(0.6);
    const foreign = otherStore.createManimCircle(0.6);
    const batch = (...members) => {
      const request = new wasm.WasmSceneMembershipBatch("add");
      for (const member of members) request.appendMobject("", member);
      return request;
    };
    const family = store.createFamily(batch());
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
    const foreignAddRejected = rejectsForeign(() => family.editMembership(batch(circle, foreign)));
    if (family.memberCount !== 0) throw new Error("failed authored batch partially committed");
    family.editMembership(batch(circle, copy, target));
    const layout = family.layout();
    const foreignFamily = otherStore.createFamily(batch(foreign));
    const foreignLayout = foreignFamily.layout();
    const foreignObjectPlacementRejected = rejectsForeign(() => layout.moveToMobject(foreign, 0, 0, 1, 1));
    const foreignFamilyPlacementRejected = rejectsForeign(() => layout.moveToFamily(foreignLayout, 0, 0, 1, 1));
    store.free();
    otherStore.free();
    // Observations retain their semantic store; one operation applies all members.
    layout.shiftBy(1, 0);
    const memberCount = family.memberCount;
    for (const handle of [layout, family, foreignLayout, foreignFamily]) handle.free();
    // Only mobject wrappers now retain the store; copy/target mutation still works.
    copy.shift(2, 0);
    target.shift(-1, 0);
    const result = {
      sameNumericId,
      foreignAddRejected,
      foreignObjectPlacementRejected,
      foreignFamilyPlacementRejected,
      identities: [circle, copy, target].map(identity),
      centers: [circle.centerX, copy.centerX, target.centerX],
      memberCount,
    };
    for (const handle of [circle, copy, target, foreign]) handle.free();
    return result;
  });
  assert.equal(handleOwnership.sameNumericId, true, "independent stores may reuse numeric IDs");
  assert.equal(handleOwnership.foreignAddRejected, true);
  assert.equal(handleOwnership.foreignObjectPlacementRejected, true);
  assert.equal(handleOwnership.foreignFamilyPlacementRejected, true);
  assert.equal(new Set(handleOwnership.identities).size, 3, "copy/target allocate fresh identities");
  assert.deepEqual(handleOwnership.centers, [1, 3, 0], "copies/targets retain independent state");
  assert.equal(handleOwnership.memberCount, 3, "failed cross-store operations leave membership intact");

  const foundation = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    foundationSource,
  );
  assert.ok(Math.abs(foundation.duration - 2.65) < 1e-9);
  assert.equal(foundation.metrics.objectCount, 3, "introducer animations bind objects through shared membership");
  assert.ok(foundation.metrics.presentedFrames > 0);
  assert.ok(Math.abs(foundation.frame.objects[0].center[1] - 1.5) < 1e-6);
  assert.ok(foundation.frame.objects.slice(0, 2).every(object => object.reveal === 1));

  const groupFades = await page.evaluate(
    pythonSource => window.noonManimCompat.runLive(pythonSource), groupFadeSource,
  );
  assert.equal(groupFades.metrics.objectCount, 0, "family FadeOut detaches the shared root");
  assert.ok(groupFades.metrics.presentedFrames > 0);
  assert.equal(groupFades.duration, 0.5);

  const uncreate = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    uncreateSource,
  );
  assert.equal(uncreate.duration, 4, "Uncreate options must preserve sequential authored timing");
  assert.equal(uncreate.metrics.objectCount, 1, "only remover=False target should remain live");
  assert.ok(uncreate.metrics.presentedFrames > 0, "shared Uncreate options must present");

  const phaseB = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    phaseBSource,
  );
  assert.equal(phaseB.duration, 1, "membership edits preserve continuation timing");
  assert.equal(phaseB.metrics.objectCount, 0, "clear removes the final root");
  assert.ok(phaseB.metrics.presentedFrames > 0, "shared group membership must render");

  const defaultVmobjectStyle = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    defaultVmobjectStyleSource,
  );
  assert.equal(defaultVmobjectStyle.metrics.objectCount, 3);
  assert.ok(defaultVmobjectStyle.metrics.presentedFrames > 0);
  const defaultStyle = defaultVmobjectStyle.frame.objects[0];
  assert.equal(defaultStyle.fill.alpha, 0);
  assert.equal(defaultStyle.stroke.alpha, 1);
  assert.ok(Math.abs(defaultStyle.stroke_width - 0.04) < 1e-7);
  assert.equal(defaultStyle.stroke_width_mode, "screen_space");
  assert.equal(defaultStyle.stroke_join, "miter");
  assert.equal(defaultStyle.stroke_cap, "butt");

  const animateParity = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    animateParitySource,
  );
  assert.ok(Math.abs(animateParity.duration - 5.6) < 1e-9);
  assert.ok(animateParity.metrics.presentedFrames > 0, "shared family animate must render");

  const queryTransforms = await page.evaluate(
    (pythonSource) => window.noonManimCompat.runLive(pythonSource),
    queryTransformSource,
  );
  assert.equal(queryTransforms.metrics.objectCount, 3);
  assert.ok(queryTransforms.metrics.presentedFrames > 0);
  assert.ok(queryTransforms.frame.objects.every(object => object.bounds.width > 0 && object.bounds.height > 0));

  // Runtime samples qualify both easing interiors and the returning endpoint.
  const ratePage = await browser.newPage();
  ratePage.on("pageerror", error => errors.push(String(error)));
  const times = [0.05, 0.1, 0.15, 0.2, 0.3, 0.5, 0.65, 0.7, 0.75, 0.8];
  await ratePage.goto(`${baseUrl}/web/manim-raster-host.html`);
  const sharedRates = await ratePage.evaluate(async ({ source, times }) => {
    await window.noonHostRaster.ready();
    await window.noonHostRaster.load(source, 1);
    const samples = [];
    for (let index = 0; index < times.length; index += 1) {
      const metrics = await window.noonHostRaster.renderThrough(index, times);
      samples.push({ metrics, frame: await window.noonHostRaster.debugFrame() });
    }
    return samples;
  }, { source: rateFunctionSource, times });
  const expectedX = [0.070103716545108, 0.5, 0.929896283454892, 1,
    0.859792566910216, 0.859792566910216, 0.5, 0, 0.5, 1];
  for (const [index, { frame, metrics }] of sharedRates.entries()) {
    assert.ok(Math.abs(frame.objects[0].center[0] - expectedX[index]) < 1e-5,
      `shared rate function at ${times[index]}s: ${frame.objects[0].center[0]} != ${expectedX[index]}`);
    assert.ok(metrics.presented && metrics.drawCalls > 0);
    assert.ok(Math.abs(metrics.time - times[index]) < 1e-9);
  }
  await ratePage.close();

  let overlapError = null;
  try {
    await page.evaluate(
      (pythonSource) => window.noonManimCompat.runLive(pythonSource),
      overlappingFamilySource,
    );
  } catch (error) {
    overlapError = String(error);
  }
  assert.match(
    overlapError ?? "",
    /ConflictingObjectDrivers/,
    "same-Text concurrent Write/Unwrite must reject atomically before lifecycle mutation",
  );

  // Plain Text Write/Unwrite and mixed ordinary compositions execute through the
  // shared semantic session. Source assertions cover membership at completion;
  // these public reports cover authored timing and stable live canvas ownership.
  const sharedText = await page.evaluate(
    (sources) => window.noonManimCompat.runLiveSources(sources),
    [
      concurrentFamilySource,
      plainTextLifecycleSource,
      mixedFamilyOrdinarySource,
      mixedFamilyOrdinaryEditedSource,
    ],
  );
  assert.equal(sharedText.sameCanvas, true, "plain Text reruns must retain the mounted canvas");
  assert.deepEqual(
    sharedText.results.map((result) => result.duration),
    [2, 2, 1, 1.5],
    "shared Text composition must preserve child-default and explicit play timing",
  );
  assert.deepEqual(
    sharedText.results.map((result) => result.metrics.objectCount),
    [2, 0, 2, 3],
    "Write/Unwrite and mixed composition must publish final scene membership",
  );
  for (const result of sharedText.results) {
    assert.ok(result.metrics.presentedFrames > 0, "shared Text composition must present");
  }

  let zError = null;
  try {
    await page.evaluate(
      (pythonSource) => window.noonManimCompat.runLive(pythonSource),
      `from noon import *\nresult = Scene()\nLine((0, 0, 1), (1, 0, 0))`,
    );
  } catch (error) {
    zError = String(error);
  }
  assert.match(zError ?? "", /z must be 0/, "non-zero z should fail explicitly");

  assert.deepEqual(errors, [], `browser errors while testing Manim compatibility:\n${errors.join("\n")}`);
  console.log(
    "Manim compatibility smoke passed: construct discovery, shape classes, scene/group semantics, callable and chained animate builders, detached animate auto-add, per-animation timing, play overrides, concurrent shared Text Write, shared Text Write/Unwrite lifecycle, mixed Text/ordinary composition, shared detached query/dimension transforms, z=0 vectors, and shared deterministic Manim rate-function lowering.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
