import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { PNG } from "pngjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
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
  await page.close();
  const cache = createPyodideResourceCache(await readFile(path.join(repoRoot, "web/python-worker.js"), "utf8"));
  const mathContext = await browser.newContext({ viewport: { width: 1000, height: 650 } });
  await cache.install(mathContext);
  await disableAuthoringJspi(mathContext);
  try {
    for (const kind of ["MathTex", "MathTypst"]) {
      for (const [forward, reverse] of [["Create", "Uncreate"], ["Write", "Unwrite"]]) {
        const source = `from noon import *
class MathReveal(Scene):
    async def construct(self):
        ${kind === "MathTex" ? "await prepare_latex()" : "pass"}
        equation = ${kind}(r"${kind === "MathTex" ? "x^2+\\frac{1}{2}" : "x^2+frac(1, 2)"}", font_size=88, color=BLUE)
        await self.play(${forward}(equation), run_time=2.0, rate_func=linear)
        await self.play(${reverse}(equation), run_time=1.0, rate_func=linear)
        await self.wait(0.25)
`;
        const mathPage = await mathContext.newPage();
        try {
          const mathErrors = [];
          mathPage.on("pageerror", error => mathErrors.push(String(error)));
          await mathPage.goto(`${baseUrl}/web/manim-raster-host.html`);
          await mathPage.waitForFunction(() => window.noonHostRaster);
          await mathPage.evaluate(source => window.noonHostRaster.load(source, 3.25), source);
          assert.equal(await mathPage.evaluate(() => window.__noonNoJspiWorkerWrapped), true);
          const times = [0, 0.5, 1, 2, 2.5, 3, 3.25], counts = [];
          let final;
          for (let index = 0; index < times.length; index++) {
            final = await mathPage.evaluate(({ index, times }) => window.noonHostRaster.renderThrough(index, times), { index, times });
            const image = PNG.sync.read(await mathPage.locator("#scene").screenshot());
            let foreground = 0;
            for (let pixel = 0; pixel < image.data.length; pixel += 4) {
              if (image.data[pixel] + image.data[pixel + 1] + image.data[pixel + 2] > 30) foreground++;
            }
            counts.push(foreground);
          }
          assert.equal(counts[0], 0, `${kind}/${forward}: equation must start hidden`);
          assert.ok(counts[1] > 0 && counts[2] > counts[1] && counts[3] > counts[1],
            `${kind}/${forward}: partial glyph/rule stages missing: ${JSON.stringify(counts)}`);
          assert.ok(counts[4] > 0 && counts[4] < counts[3], `${kind}/${reverse}: partial erasure missing`);
          assert.equal(counts[5], 0, `${kind}/${reverse}: equation was not erased`);
          assert.equal(counts[6], 0, `${kind}/${reverse}: removed equation reappeared`);
          assert.equal(final.authoredDuration, 3.25);
          assert.equal(final.objectCount, 0);
          assert.deepEqual(mathErrors, []);
          console.log(`${kind} ${forward}/${reverse} progressive pixels passed without JSPI: ${JSON.stringify(counts)}`);
        } finally { await mathPage.close(); }
      }
    }
  } finally { await mathContext.close(); }

  console.log(
    "Text/Typst/MathTypst animation smoke passed through shared live execution: scale, rotation, opacity, relative/absolute movement, FadeIn/FadeOut, re-add, and canvas reuse.",
  );
} finally {
  await browser?.close();
  await server.close();
}
