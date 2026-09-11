import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
// Pinned ManimCE v0.21 Typst examples plus the native Text surface that replaces
// Noon's temporary geometry-backed demo labels. Only the import is substituted.
const helloTextSource = `from noon import *


class HelloText(Scene):
    def construct(self):
        text = Text("Native Noon", font_size=48)
        self.add(text)
`;

const multilineTextSource = `from noon import *


class MultilineText(Scene):
    def construct(self):
        text = Text("first\\nsecond", font_size=36, line_spacing=0.5, color=YELLOW)
        self.add(text)
`;

const rangeColorSource = `from noon import *


class RangeColorText(Scene):
    def construct(self):
        text = Text(
            "Noon blue Noon",
            font="DejaVu Sans Mono",
            font_size=48,
            t2c={"Noon": RED, "[5:9]": BLUE},
        )
        parts = text.source_parts_for("Noon")
        assert [(part.source_start, part.source_end) for part in parts] == [(0, 4), (10, 14)]
        self.add(text)
`;

const nativeTextLayoutSource = `from noon import *


class NativeTextLayout(Scene):
    def construct(self):
        box = Square(side_length=2)
        label = Text("Native Noon", font_size=48)
        if label.width <= 0 or label.height <= 0:
            raise RuntimeError("native Text layout metrics must be positive")
        label.width = 2
        label.next_to(box, RIGHT, buff=0.25)
        self.add(box, label)
`;

const helloTypstSource = `from noon import *


class HelloTypst(Scene):
    def construct(self):
        text = Typst(r"*Hello* from _Typst!_", font_size=96)
        baseline = Typst(r"*Hello* from _Typst!_", font_size=48)
        assert abs(text.width / baseline.width - 2) < 1e-5
        assert abs(text.height / baseline.height - 2) < 1e-5
        self.add(text)
`;

const helloMathTypstSource = `from noon import *


class HelloMathTypst(Scene):
    def construct(self):
        equation = MathTypst(r"sum_(k=1)^n k = (n(n + 1)) / 2", font_size=72)
        baseline = MathTypst(r"sum_(k=1)^n k = (n(n + 1)) / 2", font_size=48)
        assert abs(equation.width / baseline.width - 1.5) < 1e-5
        assert abs(equation.height / baseline.height - 1.5) < 1e-5
        self.add(equation)
`;

const mixedPainterSource = `from noon import *


class MixedPainterOrder(Scene):
    def construct(self):
        self.add(Circle(radius=0.25).shift(LEFT))
        self.add(Text("middle", font_size=48))
        self.add(Square(side_length=0.5).shift(RIGHT))
`;

const cases = [
  { name: "native-text", source: helloTextSource, count: 1 },
  { name: "multiline", source: multilineTextSource, count: 1 },
  { name: "range-color", source: rangeColorSource, count: 1 },
  { name: "native-layout", source: nativeTextLayoutSource, count: 2 },
  { name: "typst", source: helloTypstSource, count: 1 },
  { name: "math-typst", source: helloMathTypstSource, count: 1 },
  { name: "mixed-painter-order", source: mixedPainterSource, count: 3 },
];
const server = await serveRepository(root, Number(process.env.NOON_TEXT_AUTHORING_PORT ?? 4187));
let browser;
const reports = [];
try {
  browser = await playwright.chromium.launch({ channel: "chromium", headless: true, args: browserArgs("webgpu") });
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
  await page.evaluate(async () => {
    const { PythonAuthoringClient } = await import("./authoring-client.js");
    const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
    const authoring = new PythonAuthoringClient();
    await authoring.ready();
    window.textAuthoringSmoke = {
      async run(source) {
        const canvas = document.createElement("canvas");
        canvas.width = 960; canvas.height = 540;
        canvas.style.width = "960px"; canvas.style.height = "540px";
        document.body.replaceChildren(canvas);
        const execution = new AuthoringExecutionClient(canvas);
        try {
          const authored = await authoring.run(source, {});
          if (!authored.semanticExecution) throw new Error("text scene did not use shared semantics");
          await execution.startSemanticExecution(authored.semanticExecution, {
            authoringClient: authoring, initiallyPaused: true, transportMode: "transferable",
          });
          await execution.advanceTo(0);
          return { kind: authored.kind, mode: execution.mode, backend: execution.rendererBackend,
            frame: await execution.debugFrame(), metrics: (await execution.metrics()).metrics };
        } finally { execution.terminate(); }
      },
      stop() { authoring.terminate(); },
    };
  });
  for (const spec of cases) {
    let timer;
    try {
      const result = await Promise.race([
        page.evaluate(source => window.textAuthoringSmoke.run(source), spec.source),
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${spec.name} timed out`)), 60000); }),
      ]);
      assert.equal(result.mode, "semantic");
      assert.equal(result.backend, "WebGPU");
      assert.equal(result.metrics.objectCount, spec.count, spec.name);
      assert.equal(result.frame.objects.length, spec.count, spec.name);
      assert.equal(result.frame.present_object_count, spec.count);
      assert.equal(new Set(result.frame.objects.map(object => object.id)).size, spec.count);
      assert.ok(result.metrics.drawCalls > 0 && result.metrics.presentedFrames > 0);
      assert.ok(result.metrics.instancesDrawn > spec.count, "text must render glyphs, not placeholder geometry");
      for (const object of result.frame.objects) {
        assert.ok(object.bounds.width > 0 && object.bounds.height > 0, "shared text layout must have positive bounds");
      }
      if (spec.name === "typst" || spec.name === "math-typst") {
        assert.deepEqual(result.frame.objects[0].fill, { red: 1, green: 1, blue: 1, alpha: 1 });
        assert.equal(result.frame.objects[0].style_opacity, 1);
      }
      if (spec.name === "native-layout") {
        const label = result.frame.objects[1];
        assert.ok(Math.abs(label.bounds.width - 2) < 1e-4);
        assert.ok(Math.abs(label.center[0] - 2.25) < 1e-4);
        assert.ok(Math.abs(label.center[1]) < 1e-5);
        assert.ok(Math.abs(label.transform.scale.x - label.transform.scale.y) < 1e-6);
      }
      if (spec.name === "mixed-painter-order") {
        assert.deepEqual(result.frame.objects.map(object => object.center[0]), [-1, 0, 1]);
      }
      reports.push({ name: spec.name, ...result });
      console.log(`PASS ${spec.name}: ${spec.count} shared objects, ${result.metrics.instancesDrawn} rendered instances`);
    } finally { clearTimeout(timer); }
  }
  await page.evaluate(() => window.textAuthoringSmoke.stop());
  assert.deepEqual(errors, []);
  if (process.env.NOON_TEXT_AUTHORING_REPORT) {
    const artifact = path.resolve(process.env.NOON_TEXT_AUTHORING_REPORT);
    await mkdir(path.dirname(artifact), { recursive: true });
    await writeFile(artifact, JSON.stringify(reports, (_key, value) => typeof value === "bigint" ? value.toString() : value, 2));
  }
} finally { await browser?.close(); await server.close(); }
