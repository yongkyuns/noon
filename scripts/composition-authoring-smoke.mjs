import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

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

// Observe behavior at composition boundaries and interiors through the normal
// shared raster host. No exported leaf tracks or second timeline model.
const endTime = [2, 2.2, 1.5, 2, 1, 1, 2.2, 0.25, 0.5].reduce((sum, duration) => sum + duration, 0);
const times = [0.5, 1.5, 2.15, 4.2, 4.45, 5.2, 5.7, 6.2, 7.2, 7.95, 8.2, 8.7, 9.05, 9.15, 9.65, 9.75, 10.9, 11.9, 12.2, endTime];
const server = await serveRepository(root, 4179);
let browser;
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  const page = await browser.newPage();
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto(`${server.baseUrl}/web/manim-raster-host.html`);
  await page.waitForFunction(() => window.noonHostRaster, null, { timeout: 30000 });
  const result = await page.evaluate(async ({ source, times }) => {
    await window.noonHostRaster.ready();
    const loaded = await window.noonHostRaster.load(source, 14);
    const samples = [];
    for (let index = 0; index < times.length; index += 1) {
      const metrics = await window.noonHostRaster.renderThrough(index, times);
      samples.push({ metrics, frame: await window.noonHostRaster.debugFrame() });
    }
    return { loaded, samples };
  }, { source, times });
  if (process.env.NOON_COMPOSITION_REPORT) {
    await writeFile(process.env.NOON_COMPOSITION_REPORT, JSON.stringify(result, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));
  }
  assert.equal(result.loaded.kind, "semantic_execution");
  assert.equal(result.loaded.rendererBackend, "WebGPU");
  const at = time => result.samples[times.indexOf(time)].frame;
  const close = (actual, expected, label) => assert.ok(Math.abs(actual - expected) < 1e-5, `${label}: ${actual} != ${expected}`);
  function center(time, index, x, y) {
    const object = at(time).objects[index];
    close(object.center[0], x, `${time} object ${index} x`);
    close(object.center[1], y, `${time} object ${index} y`);
  }
  // Unequal runtimes, default LaggedStart rescaling, repeated-target Succession,
  // nested duration scaling, and nonlinear outer easing.
  center(0.5, 0, -3, 0.25); center(0.5, 1, 0, 0);
  center(1.5, 0, -3, 0.75); center(1.5, 1, 0, -0.5);
  center(2.15, 0, -2.925, 1); center(2.15, 1, 0.025, -1); center(2.15, 2, 3, 0);
  center(4.2, 0, -2, 1); center(4.2, 1, 1, -1); center(4.2, 2, 4, 0);
  center(4.45, 2, 4, 0.5); center(5.2, 2, 3.5, 0.5); center(5.7, 2, 3, 0);
  center(6.2, 0, -2, 1.5); center(6.2, 1, 1, -0.75);
  center(7.2, 0, -2, 1); center(7.2, 1, 1, -0.25);
  center(7.95, 0, -1.929896283454892, 0); center(7.95, 1, 0.929896283454892, 0);
  center(8.2, 0, -1.5, 0); center(8.2, 1, 0.5, 0);
  center(8.7, 0, -1, 0); center(8.7, 1, 0, 0);
  assert.equal(at(9.05).present_object_count, 3);
  assert.equal(at(9.15).present_object_count, 4); center(9.15, 3, -1, -2);
  assert.equal(at(9.65).present_object_count, 4);
  center(9.75, 4, 1, -2);
  assert.ok(at(9.75).objects[5].appearance > 0);
  assert.equal(at(9.75).objects[6].appearance, 0);
  assert.ok(at(10.9).objects[5].appearance > at(10.9).objects[6].appearance);
  assert.equal(at(11.9).objects[5].appearance, 1);
  assert.equal(at(11.9).objects[6].appearance, 1);
  center(12.2, 7, 0, -2.4); center(endTime, 7, 0, -1.5);
  assert.equal(at(endTime).present_object_count, 8);
  for (const [index, sample] of result.samples.entries()) {
    close(sample.metrics.time, times[index], "exact authored sample");
    assert.ok(sample.metrics.presented && sample.metrics.drawCalls > 0);
  }
  close(result.samples.at(-1).metrics.authoredDuration, 12.65, "shared authored duration");
  assert.deepEqual(errors, []);
  console.log("PASS shared composition: 20 retained samples cover unequal runtimes, nested/nonlinear timing, Wait/Add, LaggedStartMap and repeated targets");

  // The paired diagram example needs only ordinary layout, nested groups and
  // one coherent target per motion. Qualify the Python facade through the same
  // normal host, including connection attachment at intermediate samples.
  const diagramSamples = [
    [0, 0, 1.4], [0.25, 0, 1.4], [0.75, 0.375, 1.4],
    [1.25, 0.75, 1.4], [1.5, 0.75, 1.4], [2, 0.75, 1.9],
    [2.5, 0.75, 2.4], [3, 0.75, 2.4],
  ];
  const diagramPage = await browser.newPage();
  diagramPage.on("pageerror", error => errors.push(String(error)));
  await diagramPage.goto(`${server.baseUrl}/web/manim-raster-host.html`);
  await diagramPage.waitForFunction(() => window.noonHostRaster, null, { timeout: 30000 });
  const diagram = await diagramPage.evaluate(async ({ source, times }) => {
    const loaded = await window.noonHostRaster.load(source, 3);
    const samples = [];
    for (let index = 0; index < times.length; index += 1) {
      const metrics = await window.noonHostRaster.renderThrough(index, times, {
        stopAtSourceCompletion: index === times.length - 1,
      });
      samples.push({ metrics, frame: await window.noonHostRaster.debugFrame() });
    }
    return { loaded, samples };
  }, {
    source: await readFile(path.join(root, "web/python/examples/ordinary_diagram_composition.py"), "utf8"),
    times: diagramSamples.map(([time]) => time),
  });
  assert.equal(diagram.loaded.kind, "semantic_execution");
  assert.equal(diagram.loaded.rendererBackend, "WebGPU");
  const diagramIds = diagram.samples[0].frame.objects.map(object => object.id);
  for (const [index, { metrics, frame }] of diagram.samples.entries()) {
    const [time, y, rightX] = diagramSamples[index];
    assert.equal(frame.present_object_count, 6);
    assert.deepEqual(frame.objects.map(object => object.id), diagramIds);
    for (const [leaf, x] of [[0, -1.4], [1, -1.4], [2, rightX], [3, rightX]]) {
      close(frame.objects[leaf].center[0], x, `diagram t=${time} leaf ${leaf} x`);
      close(frame.objects[leaf].center[1], y, `diagram t=${time} leaf ${leaf} y`);
    }
    const shaft = frame.objects[4].bounds;
    const tip = frame.objects[5].bounds;
    close(shaft.min[0], -0.6, `diagram t=${time} fixed attachment`);
    close(shaft.max[0], rightX - 1, `diagram t=${time} tip base`);
    close(shaft.min[1], y, `diagram t=${time} shaft y`);
    close(shaft.height, 0, `diagram t=${time} horizontal shaft`);
    close(tip.max[0], rightX - 0.8, `diagram t=${time} moving attachment`);
    close(tip.width, 0.2, `diagram t=${time} tip length`);
    close(metrics.time, time, "diagram authored sample");
    assert.ok(metrics.presented && metrics.drawCalls > 0);
  }
  close(diagram.samples.at(-1).metrics.authoredDuration, 3, "diagram duration");
  assert.deepEqual(errors, []);
  await diagramPage.close();
  console.log("PASS ordinary diagram composition: eight retained samples preserve labels, identities and arrow attachments");
} finally { await browser?.close(); await server.close(); }

// Qualify Noon's independent retained channels separately from Manim point mutation.
await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_MORPH_REVEAL_ARTIFACTS ?? "morph-reveal-artifacts",
  cases: [1.5, 2.5, 3].map(sampleTime => ({
    id: `morph-reveal-${sampleTime}`, file: "ordinary_morph_reveal.py",
    factory: "createDirectMorphRevealRenderer", playback: "live", objectCount: 1, duration: 3, sampleTime,
  })),
});
