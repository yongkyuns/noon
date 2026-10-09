import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { browserArgs } from "./manim-raster-support.mjs";
import { serveRepository } from "./browser-test-server.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const port = 4191;
const server = await serveRepository(repoRoot, port, { crossOriginIsolated: true });
const baseUrl = server.baseUrl;

const textAnimateSource = `
from noon import *

class TypedTextAnimate(Scene):
    def construct(self):
        label = Text("Animate", font_size=48)

        self.play(
            label.animate(run_time=2.0, rate_func=linear)
                .scale(2.0)
                .shift(RIGHT)
                .rotate(PI / 2)
                .set_opacity(0.25)
        )
        assert abs(label.get_center()[0] - 1.0) < 1e-5
        assert abs(label.get_center()[1]) < 1e-5
        self.play(
            label.animate.scale(0.5).shift(UP).rotate(-PI / 4).set_opacity(0.75),
            run_time=1.0,
        )
        assert abs(label.get_center()[0] - 1.0) < 1e-5
        assert abs(label.get_center()[1] - 1.0) < 1e-5
        self.play(label.animate.move_to(2 * LEFT), run_time=1.0)
        assert abs(label.get_center()[0] + 2.0) < 1e-5
        assert abs(label.get_center()[1]) < 1e-5
        assert label in self.mobjects

        invalid = Text("Invalid", font_size=36)
        try:
            self.play(invalid.animate.rotate(float("nan")), run_time=0.25)
            raise AssertionError("non-finite Text animation must fail")
        except ValueError:
            pass
        assert invalid not in self.mobjects
        assert abs(invalid.get_center()[0]) < 1e-5
        assert abs(invalid.get_center()[1]) < 1e-5
`;

const textFadeSource = `
from noon import *

class TypedTextFade(Scene):
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
        assert abs(label.get_center()[0] + 1.0) < 1e-5
        assert abs(label.get_center()[1] - 1.0) < 1e-5
`;

const typstAnimationSource = `
from noon import *

class TypstAnimation(Scene):
    def construct(self):
        label = Typst(r"*Typed* Typst", font_size=56).shift(2 * LEFT)
        equation = MathTypst(r"x^2 + y^2 = 1", font_size=52).shift(2 * RIGHT)

        self.play(
            FadeIn(label, shift=DOWN),
            FadeIn(equation, shift=UP),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == [label, equation]

        self.play(
            label.animate.shift(RIGHT).set_opacity(0.4),
            equation.animate.shift(LEFT).rotate(PI / 8).scale(0.75),
            run_time=1.0,
            rate_func=linear,
        )
        assert abs(label.get_center().x + 1.0) < 1e-5
        assert abs(equation.get_center().x - 1.0) < 1e-5

        self.play(
            FadeOut(label, shift=UP),
            FadeOut(equation, shift=DOWN),
            run_time=1.0,
            rate_func=linear,
        )
        assert self.mobjects == []
        self.wait(1.0)
`;

let browser = null;
try {
  browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: browserArgs("webgpu"),
  });
  const page = await browser.newPage({ viewport: { width: 800, height: 500 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(`pageerror: ${error}`));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(`console: ${message.text()}`);
  });

  await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
  await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
  // The source runner owns the test lifecycle. Do not pre-run unrelated
  // animated readiness probes on the same long-lived Python worker.

  const result = await page.evaluate(
    (sources) => window.noonManimCompat.runLiveSources(sources),
    [textAnimateSource, textFadeSource, typstAnimationSource],
  );
  assert.equal(result.sameCanvas, true, "Text rerun must retain the mounted canvas");
  const expectedFinalCounts = [1, 1, 0];
  for (const [index, execution] of result.results.entries()) {
    assert.equal(execution.duration, 4, `Text case ${index}: authored duration`);
    assert.equal(
      execution.metrics.objectCount,
      expectedFinalCounts[index],
      `Text case ${index}: final membership`,
    );
    assert.ok(execution.metrics.presentedFrames > 0, `Text case ${index}: no rendered frame`);
  }
  assert.deepEqual(errors, [], `browser errors while testing typed Text animation:\n${errors.join("\n")}`);

  console.log(
    "Text/Typst/MathTypst animation smoke passed through shared live execution: scale, rotation, opacity, relative/absolute movement, FadeIn/FadeOut, re-add, and canvas reuse.",
  );
} finally {
  await browser?.close();
  await server.close();
}
