// Actual shared-Rust/Python source replacement, including the ordinary live transition.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { qualifyPairedAuthoring, qualifyPythonPlayback } from "./paired-authoring-qualification.mjs";

const artifactDirectory = process.env.NOON_CODE_SOURCE_ARTIFACTS ?? "code-source-artifacts";
const source = await readFile(new URL("../web/python/examples/code_source_update.py", import.meta.url), "utf8");
const beforeFrames = new Map();
const afterPixels = new Map();

await qualifyPairedAuthoring({
  artifactDirectory,
  cases: [
    { id: "code-source-before", source, scene: "CodeSourceBefore",
      factory: "createCodeSourceRenderer", factoryArgs: [false], objectCount: 25 },
    { id: "code-source-after", source, scene: "CodeSourceAfter",
      factory: "createCodeSourceRenderer", factoryArgs: [true], objectCount: 25 },
    { id: "code-source-transition", source, scene: "CodeSourceUpdate",
      factory: "createCodeSourceUpdateRenderer", playback: "live",
      duration: 1.15, sampleTime: 0.575, boundaries: [0.2, 0.95] },
    { id: "code-source-completed", source, scene: "CodeSourceUpdate",
      factory: "createCodeSourceUpdateRenderer", playback: "live",
      duration: 1.15, sampleTime: 1.15, boundaries: [0.2, 0.95], stopAtSourceCompletion: true },
  ],
  async qualifyPixels({ fixture, backend, rust }) {
    if (fixture.id === "code-source-transition") {
      assert.ok(afterPixels.has(backend));
      assert.ok(!rust.data.equals(beforeFrames.get(backend).pixels.data), "the row transition must be visible");
      assert.ok(!rust.data.equals(afterPixels.get(backend).data), "the transition must have an intermediate state");
      return { intermediatePixelsDifferFromBothEndpoints: true };
    }
    if (fixture.id === "code-source-completed") {
      assert.ok(rust.data.equals(afterPixels.get(backend).data),
        "live completion must equal the independently admitted final source");
      return { completedPixelsEqualColdSource: true };
    }
    const frame = JSON.parse(await readFile(path.join(artifactDirectory,
      `${fixture.id}-${backend}-rust-wasm-frame.json`), "utf8"));
    if (fixture.id === "code-source-before") {
      beforeFrames.set(backend, { frame, pixels: rust });
      return {};
    }
    const before = beforeFrames.get(backend);
    assert.ok(before, "source edit must have an independently admitted before control");
    afterPixels.set(backend, rust);
    const previousObjects = new Map(before.frame.objects.map(object => [object.id, object]));
    const currentObjects = new Map(frame.objects.map(object => [object.id, object]));
    const removed = before.frame.objects.filter(object => !currentObjects.has(object.id));
    const added = frame.objects.filter(object => !previousObjects.has(object.id));
    assert.equal(removed.length, 1, "only the requested source row loses membership");
    assert.equal(added.length, 1, "only the requested source row gets a fresh identity");
    let stableObjects = 0;
    for (const object of frame.objects) {
      const previous = previousObjects.get(object.id);
      if (previous) {
        assert.deepEqual(object, previous, "other rows, gutter, panel and highlight must keep the same state");
        stableObjects++;
      }
    }
    assert.equal(stableObjects, 24);
    // Read the allocated row from the untouched before scene. Using only the
    // union of old/new glyph bounds would also accept a misplaced replacement
    // because its incorrect position could enlarge the permitted region.
    const yellow = [0xf7, 0xd9, 0x6f].map(channel => channel / 255);
    const highlights = before.frame.objects.filter(object => object.present && object.fill &&
      Math.abs(object.fill.alpha - 0.2) < 1e-6 &&
      [object.fill.red, object.fill.green, object.fill.blue]
        .every((channel, index) => Math.abs(channel - yellow[index]) < 1e-6));
    assert.equal(highlights.length, 1, "the before scene must contain one default translucent row highlight");
    const highlight = highlights[0];
    assert.deepEqual(currentObjects.get(highlight.id), highlight,
      "source replacement must keep the independently allocated row highlight unchanged");
    assert.ok(highlight.bounds, "the allocated row must have render bounds");
    assert.deepEqual(frame.camera, before.frame.camera, "source replacement must keep the camera fixed");
    const allocatedBounds = {
      left: highlight.bounds.min[0], right: highlight.bounds.max[0],
      bottom: highlight.bounds.min[1], top: highlight.bounds.max[1],
    };
    const inkBounds = {
      left: Math.min(removed[0].bounds.min[0], added[0].bounds.min[0]),
      right: Math.max(removed[0].bounds.max[0], added[0].bounds.max[0]),
      bottom: Math.min(removed[0].bounds.min[1], added[0].bounds.min[1]),
      top: Math.max(removed[0].bounds.max[1], added[0].bounds.max[1]),
    };
    const unitsPerPixel = frame.camera.height / rust.height;
    const margin = 2 * unitsPerPixel;
    assert.ok(added[0].bounds.min[0] >= allocatedBounds.left - margin &&
      added[0].bounds.max[0] <= allocatedBounds.right + margin &&
      added[0].bounds.min[1] >= allocatedBounds.bottom - margin &&
      added[0].bounds.max[1] <= allocatedBounds.top + margin,
    "the replacement glyph bounds must stay inside the original allocated row");
    let changedPixels = 0;
    for (let offset = 0; offset < rust.data.length; offset += 4) {
      if (rust.data.readUInt32LE(offset) === before.pixels.data.readUInt32LE(offset)) continue;
      const index = offset / 4;
      const x = frame.camera.center[0] + (index % rust.width + 0.5 - rust.width / 2) * unitsPerPixel;
      const y = frame.camera.center[1] + (rust.height / 2 - Math.floor(index / rust.width) - 0.5) * unitsPerPixel;
      assert.ok(x >= allocatedBounds.left - margin && x <= allocatedBounds.right + margin &&
        y >= allocatedBounds.bottom - margin && y <= allocatedBounds.top + margin,
      `source edit changed a pixel outside the original allocated row at (${x}, ${y})`);
      assert.ok(x >= inkBounds.left - margin && x <= inkBounds.right + margin &&
        y >= inkBounds.bottom - margin && y <= inkBounds.top + margin,
      `source edit changed a pixel outside the edited glyph row at (${x}, ${y})`);
      changedPixels++;
    }
    assert.ok(changedPixels > 100, "the padded declaration must change real glyph pixels");
    // These are explicit author-supplied named colors, so this also catches a
    // wrapper that silently drops token/category options before Rust admission.
    const colors = { teal: [0x5c, 0xd0, 0xb3], purple: [0x9a, 0x72, 0xac],
      gold: [0xf0, 0xac, 0x5f], pink: [0xd1, 0x47, 0xbd] };
    const colorPixels = Object.fromEntries(Object.keys(colors).map(name => [name, 0]));
    for (let offset = 0; offset < rust.data.length; offset += 4) {
      for (const [name, rgb] of Object.entries(colors)) {
        if (rgb.every((value, channel) => Math.abs(rust.data[offset + channel] - value) <= 8)) {
          colorPixels[name]++;
        }
      }
    }
    for (const [name, count] of Object.entries(colorPixels)) {
      assert.ok(count > 8, `shared ${name} token/category color must reach rendered glyphs`);
    }
    return { stableObjects, changedPixels, changesConfinedToEditedRow: true,
      originalAllocatedRowUnchanged: true, replacementFitsOriginalAllocatedRow: true, colorPixels };
  },
  async prepareContext(context) {
    if (!process.env.NOON_PYODIDE_ROOT) return;
    const worker = await readFile(new URL("../web/python-worker.source.js", import.meta.url), "utf8");
    const base = worker.match(/https:\/\/cdn\.jsdelivr\.net\/pyodide\/v[^/]+\/full\//)?.[0];
    assert.ok(base);
    await context.route(url => url.href.startsWith(base), async route => {
      const name = new URL(route.request().url()).pathname.split("/").at(-1);
      const body = await readFile(path.join(process.env.NOON_PYODIDE_ROOT, name));
      const contentType = name.endsWith(".mjs") ? "application/javascript"
        : name.endsWith(".wasm") ? "application/wasm" : "application/octet-stream";
      await route.fulfill({ body, contentType });
    });
  },
  async qualifyLifecycle(context, baseUrl, backend) {
    const contract = await readFile(new URL("./code-source-authoring-contract.py", import.meta.url), "utf8");
    const result = await qualifyPythonPlayback(context, baseUrl, contract, [0, 0.05, 0.8]);
    assert.equal(result.rendererBackend, backend);
    assert.equal(result.presented, true);
    assert.ok(Math.abs(result.authoredDuration - 0.8) < 1e-6);
    return result;
  },
});
