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
        assert circle.style["stroke_join"] == "miter"
        assert circle.style["stroke_cap"] == "butt"
        assert circle.style["fill"]["red"] == 1.0
        assert circle.style["stroke"]["red"] == 1.0

        explicit = Square(stroke_width=10)
        assert abs(explicit.style["stroke_width"] - 0.10) < 1e-9
        explicit.set_stroke(width=20)
        assert abs(explicit.style["stroke_width"] - 0.20) < 1e-9

        filled = Circle(fill_color=PINK, fill_opacity=0.5)
        assert abs(filled.get_fill_opacity() - 0.5) < 1e-12
        assert abs(filled.style["stroke_width"] - 0.04) < 1e-9
        self.add(circle, explicit, filled)
`;

const styleSource = `
from noon import *

class IndependentStyleOpacity(Scene):
    def construct(self):
        square = Square(
            side_length=0.8,
            fill_color=BLUE,
            fill_opacity=0.25,
            stroke_color=RED,
            stroke_opacity=0.6,
            stroke_width=0.06,
        )
        assert abs(square.get_fill_opacity() - 0.25) < 1e-9
        assert abs(square.get_stroke_opacity() - 0.6) < 1e-9

        square.set_fill(opacity=0.75)
        assert abs(square.get_fill_opacity() - 0.75) < 1e-9
        assert abs(square.get_stroke_opacity() - 0.6) < 1e-9

        square.set_stroke(opacity=0.4)
        assert abs(square.get_fill_opacity() - 0.75) < 1e-9
        assert abs(square.get_stroke_opacity() - 0.4) < 1e-9

        square.set_opacity(0.2)
        assert abs(square.get_fill_opacity() - 0.2) < 1e-9
        assert abs(square.get_stroke_opacity() - 0.2) < 1e-9
        self.add(square)

        target = square.copy().set_fill(PINK, opacity=0.8).set_stroke(GREEN, opacity=0.3)
        self.play(Transform(square, target), run_time=0.4, rate_func=linear)
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
        self.add(first)
        self.play(Uncreate(first), run_time=2.0, rate_func=rush_into)
        assert first not in self.mobjects

        kept = Circle(radius=0.25, color=PINK)
        self.add(kept)
        self.play(Uncreate(kept, remover=False), run_time=1.0)
        assert kept in self.mobjects

        forward = Square(side_length=0.4, color=GREEN)
        self.add(forward)
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
  assert.equal(defaultStyle.stroke_join, "miter");
  assert.equal(defaultStyle.stroke_cap, "butt");

  const style = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    styleSource,
  );
  assert.equal(style.kind, "scene_document");
  assert.equal(style.document.objects.length, 1);
  const baseStyle = style.document.objects[0].style;
  assert.equal(baseStyle.opacity, 1, "independent layer opacity should not consume overall opacity");
  assert.equal(baseStyle.fill.alpha, 0.2);
  assert.equal(baseStyle.stroke.alpha, 0.2);
  const styleTransform = style.document.tracks.find((track) => track.property === "transform");
  assert.equal(styleTransform.values.object.to.style.fill.alpha, 0.8);
  assert.equal(styleTransform.values.object.to.style.stroke.alpha, 0.3);

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
    "Manim compatibility smoke passed: construct discovery, shape classes, scene/group semantics, callable and chained animate builders, detached animate auto-add, per-animation timing, play overrides, independent fill/stroke opacity, shared detached query/dimension transforms, z=0 vectors, and shared deterministic Manim rate-function lowering.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
