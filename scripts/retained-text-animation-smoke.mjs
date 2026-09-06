import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4191;
const baseUrl = `http://127.0.0.1:${port}`;
const retainedScalarTolerance = 1e-6;

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
  throw new Error(`retained text animation smoke server did not start: ${lastError}\n${serverOutput}`);
}

const retainedAnimateSource = `
from noon import *

class RetainedAnimate(Scene):
    def construct(self):
        label = Text("Animate", font_size=48)

        self.play(
            label.animate(run_time=2.0, rate_func=linear)
                .scale(2.0)
                .shift(RIGHT)
                .rotate(PI / 2)
                .set_opacity(0.25)
        )
        self.play(
            label.animate.scale(0.5).shift(UP).rotate(-PI / 4).set_opacity(0.75),
            run_time=1.0,
        )
        self.play(label.animate.move_to(2 * LEFT), run_time=1.0)

        assert label in self.mobjects

        unsupported = Text("Unsupported", font_size=36)
        try:
            self.play(unsupported.animate.set_color(RED), run_time=0.25)
            raise AssertionError("unsupported retained animate.set_color must fail")
        except NotImplementedError as error:
            assert "animate.set_color" in str(error)
        assert unsupported not in self.mobjects
`;

const retainedFadeSource = `
from noon import *

class RetainedFade(Scene):
    def construct(self):
        label = Text("Fade", font_size=48).shift(LEFT)

        self.wait(0.5)
        assert label not in self.mobjects

        self.play(
            FadeIn(label, shift=DOWN, scale=0.5),
            run_time=1.0,
            rate_func=linear,
        )
        assert label in self.mobjects

        self.play(
            FadeOut(label, shift=2 * RIGHT, scale=1.5),
            run_time=1.0,
        )
        assert label not in self.mobjects

        self.wait(0.5)
        self.add(label)
        assert label in self.mobjects
        self.play(label.animate.shift(UP), run_time=1.0)
        assert label in self.mobjects
`;

function assertNear(actual, expected, message) {
  assert.ok(
    Math.abs(actual - expected) <= retainedScalarTolerance,
    `${message}: expected ${expected}, got ${actual}`,
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
    retainedAnimateSource,
  );

  assert.equal(result.kind, "scene_document");
  assert.equal(
    result.document.objects.length,
    0,
    "retained Text.animate must not create legacy placeholder geometry",
  );
  const retained = await page.evaluate(
    ({ result, label }) => window.noonManimCompat.retainedTextView(result, label),
    { result, label: "retained animation" },
  );
  assert.ok(retained, "retained animation must emit canonical text content");
  assert.equal(retained.objects.length, 1);
  assert.equal(retained.objects[0].text.source, "Animate");

  const tracks = retained.tracks ?? [];
  const scaleTracks = tracks.filter((track) => track.property === "scale");
  const positionTracks = tracks.filter((track) => track.property === "position");
  const rotationTracks = tracks.filter((track) => track.property === "rotation");
  const opacityTracks = tracks.filter((track) => track.property === "opacity");
  assert.equal(scaleTracks.length, 2, "two scale calls must emit two retained scale tracks");
  assert.equal(
    positionTracks.length,
    3,
    "two relative shifts and one absolute move_to must emit three retained position tracks",
  );
  assert.equal(rotationTracks.length, 2, "two rotate calls must emit two retained rotation tracks");
  assert.equal(opacityTracks.length, 2, "two opacity calls must emit two retained opacity tracks");

  assert.deepEqual(scaleTracks[0].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 2, y: 2 },
  });
  assert.equal(scaleTracks[0].timing.start_time, 0);
  assert.equal(scaleTracks[0].timing.duration, 2);
  assert.equal(scaleTracks[0].timing.easing, "linear");

  assert.deepEqual(scaleTracks[1].values.vec2, {
    from: { x: 2, y: 2 },
    to: { x: 1, y: 1 },
  });
  assert.equal(scaleTracks[1].timing.start_time, 2);
  assert.equal(scaleTracks[1].timing.duration, 1);
  assert.equal(scaleTracks[1].timing.easing, "smooth");

  assert.deepEqual(positionTracks[0].values.vec2, {
    from: { x: 0, y: 0 },
    to: { x: 1, y: 0 },
  });
  assert.equal(positionTracks[0].timing.start_time, 0);
  assert.equal(positionTracks[0].timing.duration, 2);
  assert.equal(positionTracks[0].timing.easing, "linear");

  assert.deepEqual(positionTracks[1].values.vec2, {
    from: { x: 1, y: 0 },
    to: { x: 1, y: 1 },
  });
  assert.equal(positionTracks[1].timing.start_time, 2);
  assert.equal(positionTracks[1].timing.duration, 1);
  assert.equal(positionTracks[1].timing.easing, "smooth");

  assert.deepEqual(positionTracks[2].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: -2, y: 0 },
  });
  assert.equal(positionTracks[2].timing.start_time, 3);
  assert.equal(positionTracks[2].timing.duration, 1);
  assert.equal(positionTracks[2].timing.easing, "smooth");

  // Retained transform scalars are Rust f32 values serialized through the WASM handle.
  assertNear(rotationTracks[0].values.scalar.from, 0, "first rotation source");
  assertNear(rotationTracks[0].values.scalar.to, Math.PI / 2, "first rotation target");
  assert.equal(rotationTracks[0].timing.start_time, 0);
  assert.equal(rotationTracks[0].timing.duration, 2);
  assert.equal(rotationTracks[0].timing.easing, "linear");

  assertNear(rotationTracks[1].values.scalar.from, Math.PI / 2, "second rotation source");
  assertNear(rotationTracks[1].values.scalar.to, Math.PI / 4, "second rotation target");
  assert.equal(rotationTracks[1].timing.start_time, 2);
  assert.equal(rotationTracks[1].timing.duration, 1);
  assert.equal(rotationTracks[1].timing.easing, "smooth");

  assertNear(opacityTracks[0].values.scalar.from, 1, "first opacity source");
  assertNear(opacityTracks[0].values.scalar.to, 0.25, "first opacity target");
  assert.equal(opacityTracks[0].timing.start_time, 0);
  assert.equal(opacityTracks[0].timing.duration, 2);
  assert.equal(opacityTracks[0].timing.easing, "linear");

  assertNear(opacityTracks[1].values.scalar.from, 0.25, "second opacity source");
  assertNear(opacityTracks[1].values.scalar.to, 0.75, "second opacity target");
  assert.equal(opacityTracks[1].timing.start_time, 2);
  assert.equal(opacityTracks[1].timing.duration, 1);
  assert.equal(opacityTracks[1].timing.easing, "smooth");

  assert.equal(result.duration, 4);

  const fadeResult = await page.evaluate(
    (source) => window.noonManimCompat.run(source),
    retainedFadeSource,
  );
  assert.equal(fadeResult.kind, "scene_document");
  assert.equal(
    fadeResult.document.objects.length,
    0,
    "retained Text fades must not create legacy placeholder geometry",
  );
  const fadeRetained = await page.evaluate(
    ({ result, label }) => window.noonManimCompat.retainedTextView(result, label),
    { result: fadeResult, label: "retained fade" },
  );
  assert.ok(fadeRetained, "retained fade must emit canonical text content");
  assert.equal(fadeRetained.objects.length, 1);
  assert.equal(fadeRetained.objects[0].text.source, "Fade");
  assert.deepEqual(fadeRetained.objects[0].text.transform.translation, {
    x: -1,
    y: 0,
  });
  assert.deepEqual(fadeRetained.objects[0].text.transform.scale, {
    x: 1,
    y: 1,
  });

  const fadeTracks = fadeRetained.tracks ?? [];
  const fadePresence = fadeTracks.filter((track) => track.property === "presence");
  const fadeAppearance = fadeTracks.filter((track) => track.property === "appearance");
  const fadePosition = fadeTracks.filter((track) => track.property === "position");
  const fadeScale = fadeTracks.filter((track) => track.property === "scale");

  assert.deepEqual(
    fadePresence.map((track) => ({
      values: track.values.bool,
      start: track.timing.start_time,
      duration: track.timing.duration,
      easing: track.timing.easing,
    })),
    [
      {
        values: { from: false, to: true },
        start: 0.5,
        duration: 0,
        easing: "linear",
      },
      {
        values: { from: true, to: false },
        start: 2.5,
        duration: 0,
        easing: "linear",
      },
      {
        values: { from: false, to: true },
        start: 3,
        duration: 0,
        easing: "linear",
      },
    ],
    "retained fades and later animate must use explicit lifecycle presence transitions",
  );

  assert.equal(fadeAppearance.length, 3);
  assertNear(fadeAppearance[0].values.scalar.from, 0, "FadeIn appearance source");
  assertNear(fadeAppearance[0].values.scalar.to, 1, "FadeIn appearance target");
  assert.equal(fadeAppearance[0].timing.start_time, 0.5);
  assert.equal(fadeAppearance[0].timing.duration, 1);
  assert.equal(fadeAppearance[0].timing.easing, "linear");

  assertNear(fadeAppearance[1].values.scalar.from, 1, "FadeOut appearance source");
  assertNear(fadeAppearance[1].values.scalar.to, 0, "FadeOut appearance target");
  assert.equal(fadeAppearance[1].timing.start_time, 1.5);
  assert.equal(fadeAppearance[1].timing.duration, 1);
  assert.equal(fadeAppearance[1].timing.easing, "smooth");

  assertNear(fadeAppearance[2].values.scalar.from, 0, "FadeOut cleanup appearance source");
  assertNear(fadeAppearance[2].values.scalar.to, 1, "FadeOut cleanup appearance target");
  assert.equal(fadeAppearance[2].timing.start_time, 2.5);
  assert.equal(fadeAppearance[2].timing.duration, 0);
  assert.equal(fadeAppearance[2].timing.easing, "linear");

  assert.deepEqual(fadePosition[0].values.vec2, {
    from: { x: -1, y: 1 },
    to: { x: -1, y: 0 },
  });
  assert.equal(fadePosition[0].timing.start_time, 0.5);
  assert.equal(fadePosition[0].timing.duration, 1);
  assert.equal(fadePosition[0].timing.easing, "linear");

  assert.deepEqual(fadePosition[1].values.vec2, {
    from: { x: -1, y: 0 },
    to: { x: 1, y: 0 },
  });
  assert.equal(fadePosition[1].timing.start_time, 1.5);
  assert.equal(fadePosition[1].timing.duration, 1);
  assert.equal(fadePosition[1].timing.easing, "smooth");

  assert.equal(fadePosition.length, 4);
  assert.deepEqual(fadePosition[2].values.vec2, {
    from: { x: 1, y: 0 },
    to: { x: -1, y: 0 },
  });
  assert.equal(fadePosition[2].timing.start_time, 2.5);
  assert.equal(fadePosition[2].timing.duration, 0);
  assert.equal(fadePosition[2].timing.easing, "linear");

  assert.deepEqual(fadePosition[3].values.vec2, {
    from: { x: -1, y: 0 },
    to: { x: -1, y: 1 },
  });
  assert.equal(fadePosition[3].timing.start_time, 3);
  assert.equal(fadePosition[3].timing.duration, 1);
  assert.equal(fadePosition[3].timing.easing, "smooth");

  assert.deepEqual(fadeScale[0].values.vec2, {
    from: { x: 0.5, y: 0.5 },
    to: { x: 1, y: 1 },
  });
  assert.equal(fadeScale[0].timing.start_time, 0.5);
  assert.equal(fadeScale[0].timing.duration, 1);
  assert.equal(fadeScale[0].timing.easing, "linear");

  assert.deepEqual(fadeScale[1].values.vec2, {
    from: { x: 1, y: 1 },
    to: { x: 1.5, y: 1.5 },
  });
  assert.equal(fadeScale[1].timing.start_time, 1.5);
  assert.equal(fadeScale[1].timing.duration, 1);
  assert.equal(fadeScale[1].timing.easing, "smooth");

  assert.deepEqual(fadeScale[2].values.vec2, {
    from: { x: 1.5, y: 1.5 },
    to: { x: 1, y: 1 },
  });
  assert.equal(fadeScale[2].timing.start_time, 2.5);
  assert.equal(fadeScale[2].timing.duration, 0);
  assert.equal(fadeScale[2].timing.easing, "linear");

  assert.equal(fadeResult.duration, 4);

  const wire = JSON.stringify(retained);
  const fadeWire = JSON.stringify(fadeRetained);
  for (const forbidden of ["glyph", "font_bytes", "svg", "geometry", "atlas"]) {
    assert.ok(!wire.includes(forbidden), `retained animation wire must not contain ${forbidden}`);
    assert.ok(!fadeWire.includes(forbidden), `retained fade wire must not contain ${forbidden}`);
  }
  assert.deepEqual(errors, [], `browser errors while testing retained Text animation:\n${errors.join("\n")}`);

  console.log(
    "Retained Text animation smoke passed: scale, position, rotation, opacity, and FadeIn/FadeOut lower to source-level retained tracks; FadeOut cleanup restores canonical appearance/position/scale at the removal timestamp; direct Scene.add reintroduces canonical state with Presence only; and unsupported retained properties fail without legacy geometry.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
