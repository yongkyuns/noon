import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { PNG } from "pngjs";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

const artifactDirectory = process.env.NOON_CODE_ARTIFACTS ?? "code-artifacts";

async function qualifyOpacity(context, baseUrl, expectedBackend) {
  const page = await context.newPage();
  const source = `from noon import *
class CodeOpacity(Scene):
    def construct(self):
        block = Code(code_string='return 42\\n\\tprint("ready")', language="python",
                     formatter_style="friendly", background="window")
        self.add(block)
        self.wait(0.05)
        block.set_opacity(0)
        self.wait(0.05)
        block.set_opacity(1)
        self.wait(0.05)
`;
  try {
    page.setDefaultTimeout(90_000);
    await page.goto(`${baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    await page.evaluate(source => window.noonHostRaster.load(source, 1), source);
    const frameTimes = [0, 0.025, 0.05, 0.075, 0.1, 0.125, 0.15];
    const frames = [];
    for (const [label, index] of [["before", 1], ["hidden", 3], ["restored", 5]]) {
      const metrics = await page.evaluate(({ index, frameTimes }) =>
        window.noonHostRaster.renderThrough(index, frameTimes), { index, frameTimes });
      assert.equal(metrics.rendererBackend, expectedBackend);
      assert.equal(metrics.presented, true);
      const png = PNG.sync.read(await page.locator("#scene").screenshot({
        path: path.join(artifactDirectory, `code-opacity-${expectedBackend}-${label}.png`),
      }));
      frames.push(png);
    }
    const foreground = png => {
      let pixels = 0;
      for (let i = 0; i < png.data.length; i += 4) {
        if (png.data[i] || png.data[i + 1] || png.data[i + 2]) pixels++;
      }
      return pixels;
    };
    assert.ok(foreground(frames[0]) > 1000, "opacity test must begin with rendered Code");
    assert.equal(foreground(frames[1]), 0, "opacity zero must hide text, numbers, panel and window controls");
    assert.ok(frames[0].data.equals(frames[2].data), "restoring opacity must recover identical highlighted pixels");
    return { hiddenForegroundPixels: 0, restoredPixelsIdentical: true };
  } finally {
    await page.close();
  }
}

// These pixels qualify one shared scene across Rust/WASM and the production
// Python worker. They do not claim Pygments or Manim raster-oracle equivalence.
await qualifyPairedAuthoring({
  artifactDirectory,
  cases: [{
    id: "code-block", file: "code_block.py", scene: "CodeBlocks",
    factory: "createCodeRenderer", objectCount: 24,
  }],
  async prepareContext(context) {
    if (!process.env.NOON_PYODIDE_ROOT) return;
    const worker = await readFile(new URL("../web/python-worker.source.js", import.meta.url), "utf8");
    const base = worker.match(/https:\/\/cdn\.jsdelivr\.net\/pyodide\/v[^/]+\/full\//)?.[0];
    assert.ok(base, "worker must pin Pyodide for dependency-cache qualification");
    await context.route(url => url.href.startsWith(base), async route => {
      const name = new URL(route.request().url()).pathname.split("/").at(-1);
      const body = await readFile(path.join(process.env.NOON_PYODIDE_ROOT, name));
      const contentType = name.endsWith(".mjs") ? "application/javascript"
        : name.endsWith(".wasm") ? "application/wasm" : "application/octet-stream";
      await route.fulfill({ body, contentType });
    });
  },
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const source = await readFile(new URL("./code-authoring-contract.py", import.meta.url), "utf8");
    const result = await qualifyPythonPlayback(context, baseUrl, source, [0, 0.05, 0.1]);
    assert.equal(result.rendererBackend, expectedBackend);
    assert.equal(result.presented, true);
    assert.ok(Math.abs(result.authoredDuration - 0.1) < 1e-6);
    return { construction: result, opacity: await qualifyOpacity(context, baseUrl, expectedBackend) };
  },
});
