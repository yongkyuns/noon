import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { PNG } from "pngjs";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

const artifactDirectory = process.env.NOON_CODE_ARTIFACTS ?? "code-artifacts";
const rowExample = await readFile(new URL("../web/python/examples/code_line_highlight.py", import.meta.url), "utf8");
const rowControl = rowExample.replace(/^        code\.highlight_lines\(.*\)\n/gm, "");
assert.notEqual(rowControl, rowExample, "row control must remove the two decorations");
const rowControls = new Map();

async function qualifyOpacity(context, baseUrl, expectedBackend) {
  const page = await context.newPage();
  const source = `from noon import *
class CodeOpacity(Scene):
    def construct(self):
        block = Code(code_string='return 42\\n\\tprint("ready")', language="python",
                     formatter_style="friendly", background="window")
        block.highlight_lines(0, fill_opacity=1)
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
    assert.equal(foreground(frames[1]), 0, "opacity zero must hide text, numbers, panel, window controls and row highlights");
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
  cases: [
    {
      id: "code-block", file: "code_block.py", scene: "CodeBlocks",
      factory: "createCodeRenderer", objectCount: 24,
    },
    {
      id: "code-row-control", source: rowControl, scene: "CodeLineHighlights",
      factory: "createCodeLineHighlightRenderer", factoryArgs: [false], objectCount: 14,
    },
    {
      id: "code-row-highlights", source: rowExample, scene: "CodeLineHighlights",
      factory: "createCodeLineHighlightRenderer", factoryArgs: [true], objectCount: 16,
    },
  ],
  async qualifyPixels({ fixture, backend, rust }) {
    if (fixture.id === "code-row-control") rowControls.set(backend, rust);
    if (fixture.id !== "code-row-highlights") return {};
    const control = rowControls.get(backend);
    assert.ok(control, "row highlights must be qualified with their undecorated control");
    const colors = new Map();
    for (let offset = 0; offset < control.data.length; offset += 4) {
      const key = control.data.readUInt32LE(offset);
      if ((key & 0xffffff) !== 0) colors.set(key, (colors.get(key) ?? 0) + 1);
    }
    const [panel, panelPixels] = [...colors].sort((a, b) => b[1] - a[1])[0];
    const panelBlue = (panel >>> 16) & 0xff;
    assert.ok(panelPixels > 2_000 && [panel & 0xff, (panel >>> 8) & 0xff, panelBlue].every(value => value < 128));
    let backgroundBlueGain = 0;
    for (let offset = 0; offset < rust.data.length; offset += 4) {
      if (control.data.readUInt32LE(offset) === panel) {
        backgroundBlueGain = Math.max(backgroundBlueGain, rust.data[offset + 2] - panelBlue);
      }
    }
    assert.ok(backgroundBlueGain > 0, "yellow bands must alter the panel background");
    let changed = 0;
    let glyphPixelsAboveBand = 0;
    for (let offset = 0; offset < rust.data.length; offset += 4) {
      const before = control.data.subarray(offset, offset + 4);
      const after = rust.data.subarray(offset, offset + 4);
      if (!before.equals(after)) changed++;
      const minimum = Math.min(...before.subarray(0, 3));
      if (minimum < 150 || Math.max(...before.subarray(0, 3)) - minimum > 16) continue;
      const x = (offset / 4) % rust.width;
      const y = Math.floor(offset / 4 / rust.width);
      let nearBand = false;
      for (let dy = -2; dy <= 2 && !nearBand; dy++) {
        for (let dx = -2; dx <= 2; dx++) {
          const nx = x + dx, ny = y + dy;
          if (nx < 0 || nx >= rust.width || ny < 0 || ny >= rust.height) continue;
          const neighbor = (ny * rust.width + nx) * 4;
          if (control.data.readUInt32LE(neighbor) === panel && rust.data[neighbor + 2] > panelBlue) {
            nearBand = true;
            break;
          }
        }
      }
      if (nearBand) {
        // SDF glyphs may have no fully opaque pixel at this size. An underlay
        // increases background blue because the default yellow's blue lies
        // between the dark panel and these bright glyphs. An overlay would
        // decrease glyph blue instead. This checks actual layer placement.
        assert.ok(before[2] - after[2] <= 2,
          "row bands must remain below antialiased source glyphs");
        glyphPixelsAboveBand++;
      }
    }
    assert.ok(changed > 2_000, "row bands must be visible over the opaque panel");
    assert.ok(glyphPixelsAboveBand > 20, "painter-order check must sample glyphs within the highlighted area");
    return { changedPixelsFromControl: changed, glyphPixelsAboveBand, backgroundBlueGain };
  },
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
