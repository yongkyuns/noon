import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4179;
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
  throw new Error(`Composition smoke server did not start: ${lastError}\n${serverOutput}`);
}

const source = `
from noon import *

class CompositionScene(Scene):
    def construct(self):
        a = Circle(radius=0.2, color=BLUE).shift(LEFT * 3)
        b = Square(side_length=0.4, color=PINK)
        c = Circle(radius=0.2, color=GREEN).shift(RIGHT * 3)
        self.add(a, b, c)

        # Unequal child runtimes: starts are [0, 1], maximum end is 2.
        self.play(AnimationGroup(
            a.animate(run_time=2.0, rate_func=linear).shift(UP),
            b.animate(run_time=1.0, rate_func=linear).shift(DOWN),
            lag_ratio=0.5,
        ))

        # LaggedStart uses Manim's 0.05 default and explicit total runtime rescales
        # the shared virtual schedule.
        self.play(LaggedStart(
            a.animate(run_time=1.0, rate_func=linear).shift(RIGHT),
            b.animate(run_time=1.0, rate_func=linear).shift(RIGHT),
            c.animate(run_time=1.0, rate_func=linear).shift(RIGHT),
            run_time=2.2,
        ))

        # Succession is the same shared scheduler with lag_ratio=1 and supports
        # multiple animations of one mobject because the flattened intervals do not overlap.
        self.play(Succession(
            c.animate(run_time=0.5, rate_func=linear).shift(UP),
            c.animate(run_time=1.0, rate_func=linear).shift(LEFT),
        ))

        # Nested linear compositions are recursively rescaled without introducing
        # another scheduler in Python.
        self.play(AnimationGroup(
            Succession(
                a.animate(run_time=0.5, rate_func=linear).shift(UP),
                a.animate(run_time=0.5, rate_func=linear).shift(DOWN),
            ),
            b.animate(run_time=1.0, rate_func=linear).shift(UP),
            lag_ratio=0.0,
            run_time=2.0,
        ))

        # Nonlinear outer timing is represented exactly by a shared root-to-leaf
        # CompositionTimeMap carried by each affected leaf track.
        self.play(AnimationGroup(
            a.animate(rate_func=linear).shift(RIGHT),
            b.animate(rate_func=linear).shift(LEFT),
            rate_func=smooth,
        ))

        # Manim's Wait/Add animation objects remain deterministic composition leaves.
        # Add has zero intrinsic duration, so it introduces exactly between waits.
        d = Circle(radius=0.18, color=BLUE).shift(DOWN * 2 + LEFT)
        e = Circle(radius=0.18, color=GREEN).shift(DOWN * 2 + RIGHT)
        self.play(Succession(
            Wait(0.4),
            Add(d),
            Wait(0.6),
            Add(e),
        ))

        # LaggedStartMap maps an animation constructor over direct group children and
        # reuses the same shared composition scheduler as LaggedStart.
        mapped = VGroup(
            Square(side_length=0.25, color=PINK).shift(DOWN * 3 + LEFT * 0.4),
            Square(side_length=0.25, color=PINK).shift(DOWN * 3 + RIGHT * 0.4),
        )
        self.play(LaggedStartMap(FadeIn, mapped, run_time=2.2, lag_ratio=0.1))

        # Top-level Wait advances authored time while remaining trackless; top-level
        # Add introduces immediately without consuming time.
        self.play(Wait(0.25))
        f = Square(side_length=0.2, color=YELLOW).shift(DOWN * 2.5)
        self.play(Add(f))
        self.play(f.animate(run_time=0.5, rate_func=linear).shift(UP))
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

  const result = await page.evaluate(
    (pythonSource) => window.noonManimCompat.run(pythonSource),
    source,
  );
  assert.equal(result.kind, "scene_document");
  assert.equal(errors.length, 0, errors.join("\n"));

  const transforms = result.document.tracks.filter((track) => track.property === "transform");
  assert.equal(transforms.length, 13);
  const byObject = new Map();
  for (const track of transforms) {
    const list = byObject.get(track.object) ?? [];
    list.push(track);
    byObject.set(track.object, list);
  }

  // AnimationGroup([2, 1], lag=.5): [0..2], [1..2].
  assert.equal(byObject.get(0)[0].timing.start_time, 0);
  assert.equal(byObject.get(0)[0].timing.duration, 2);
  assert.equal(byObject.get(1)[0].timing.start_time, 1);
  assert.equal(byObject.get(1)[0].timing.duration, 1);
  assert.equal(byObject.get(0)[0].time_map, undefined);

  // LaggedStart default lag=.05, run_time=2.2 over virtual duration 1.1:
  // each child duration=2, starts 2.0, 2.1, 2.2.
  assert.ok(Math.abs(byObject.get(0)[1].timing.start_time - 2.0) < 1e-9);
  assert.ok(Math.abs(byObject.get(0)[1].timing.duration - 2.0) < 1e-9);
  assert.ok(Math.abs(byObject.get(1)[1].timing.start_time - 2.1) < 1e-9);
  assert.ok(Math.abs(byObject.get(2)[0].timing.start_time - 2.2) < 1e-9);

  // Succession starts after LaggedStart ends at 4.2 and advances strictly.
  assert.ok(Math.abs(byObject.get(2)[1].timing.start_time - 4.2) < 1e-9);
  assert.ok(Math.abs(byObject.get(2)[1].timing.duration - 0.5) < 1e-9);
  assert.ok(Math.abs(byObject.get(2)[2].timing.start_time - 4.7) < 1e-9);
  assert.ok(Math.abs(byObject.get(2)[2].timing.duration - 1.0) < 1e-9);

  // Nested group total runtime=2.0. Inner Succession (0.5 + 0.5) is rescaled
  // to the parent's 2-second parallel child interval => two 1-second leaves.
  assert.ok(Math.abs(byObject.get(0)[2].timing.start_time - 5.7) < 1e-9);
  assert.ok(Math.abs(byObject.get(0)[2].timing.duration - 1.0) < 1e-9);
  assert.ok(Math.abs(byObject.get(0)[3].timing.start_time - 6.7) < 1e-9);
  assert.ok(Math.abs(byObject.get(0)[3].timing.duration - 1.0) < 1e-9);
  assert.ok(Math.abs(byObject.get(1)[2].timing.start_time - 5.7) < 1e-9);
  assert.ok(Math.abs(byObject.get(1)[2].timing.duration - 2.0) < 1e-9);

  // The nonlinear group occupies [7.7, 8.7]. Its leaves retain linear local
  // easing while the shared time map applies the outer smooth warp first.
  const nonlinearA = byObject.get(0)[4];
  const nonlinearB = byObject.get(1)[3];
  for (const track of [nonlinearA, nonlinearB]) {
    assert.ok(Math.abs(track.timing.start_time - 7.7) < 1e-9);
    assert.ok(Math.abs(track.timing.duration - 1.0) < 1e-9);
    assert.equal(track.timing.easing, "linear");
    assert.equal(track.time_map.steps.length, 1);
    assert.ok(Math.abs(track.time_map.steps[0].start) < 1e-12);
    assert.ok(Math.abs(track.time_map.steps[0].duration - 1.0) < 1e-12);
    assert.equal(track.time_map.steps[0].rate_func, "smooth");
  }

  // Wait/Add succession begins at 8.7. The zero-duration Add leaves land at
  // 9.1 and 9.7 exactly; no synthetic continuous tracks are created for them.
  const presence = result.document.tracks.filter((track) => track.property === "presence");
  const addD = presence.find((track) => track.object === 3);
  const addE = presence.find((track) => track.object === 4);
  assert.ok(Math.abs(addD.timing.start_time - 9.1) < 1e-9);
  assert.equal(addD.timing.duration, 0);
  assert.ok(Math.abs(addE.timing.start_time - 9.7) < 1e-9);
  assert.equal(addE.timing.duration, 0);

  // LaggedStartMap([1,1], lag=.1, run_time=2.2) produces two 2-second fades
  // starting at 9.7 and 9.9, matching ordinary LaggedStart timing geometry.
  const appearances = result.document.tracks.filter((track) => track.property === "appearance");
  const mappedFirst = appearances.find((track) => track.object === 5);
  const mappedSecond = appearances.find((track) => track.object === 6);
  assert.ok(Math.abs(mappedFirst.timing.start_time - 9.7) < 1e-9);
  assert.ok(Math.abs(mappedFirst.timing.duration - 2.0) < 1e-9);
  assert.ok(Math.abs(mappedSecond.timing.start_time - 9.9) < 1e-9);
  assert.ok(Math.abs(mappedSecond.timing.duration - 2.0) < 1e-9);

  // The top-level Wait pushes the final transform from 11.9 to 12.15; Add(f)
  // consumes zero time and f's transform starts at that same instant.
  const finalTransform = byObject.get(7)[0];
  assert.ok(Math.abs(finalTransform.timing.start_time - 12.15) < 1e-9);
  assert.ok(Math.abs(finalTransform.timing.duration - 0.5) < 1e-9);

  console.log("composition authoring smoke test passed");
} finally {
  if (browser !== null) await browser.close();
  server.kill("SIGTERM");
}
