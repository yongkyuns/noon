// Actual direct Rust/WASM and Python-worker pixels on WebGPU and WebGL2.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const boundaries = [0.5, 1, 1.5, 2, 3, 3.5];
const sampleTimes = [0.25, 0.75, 1.25, 1.75, 2.25, 2.5, 2.75, 3, 3.25, 3.75, 4];
const heldTimes = new Map([[0.25, 0], [0.75, 0.5], [1.25, 1], [1.75, 1.5], [3.25, 3], [3.75, 3.5]]);
const source = await readFile(new URL("../web/python/examples/image_paint.py", import.meta.url), "utf8");

// An independent nearest-texel reference, using explicit physical fit sizes.
// This does not call Noon mapping, sampling, shader, or interpolation helpers.
const texels = [
  [[255, 0, 0, 255], [0, 255, 0, 128], [0, 0, 255, 0], [255, 255, 0, 255]],
  [[0, 255, 255, 255], [255, 0, 255, 64], [255, 128, 0, 255], [32, 96, 224, 192]],
  [[128, 0, 255, 255], [255, 255, 255, 0], [0, 128, 255, 128], [255, 32, 96, 255]],
  [[16, 160, 80, 255], [96, 48, 192, 255], [224, 224, 224, 128], [0, 64, 192, 255]],
];

function reference(time, local) {
  if (time >= 3.5) return { rgba: [64, 128, 191, 255], empty: false };
  const progress = Math.max(0, Math.min(1, time - 2));
  const repeated = time >= 1.5;
  const angle = progress * Math.PI / 4;
  const x = local[0] - 0.5 - 0.2 * progress;
  const y = local[1] - 0.5 + 0.15 * progress;
  const scaleX = repeated ? 0.5 + 0.25 * progress : 1;
  const scaleY = repeated ? 0.5 + 0.1 * progress : 1;
  const placed = [
    (Math.cos(angle) * x + Math.sin(angle) * y) / scaleX + 0.5,
    (-Math.sin(angle) * x + Math.cos(angle) * y) / scaleY + 0.5,
  ];
  // A square image covers the 4x2 panel at 4x4, or fits inside it at 2x2.
  const fitWidth = time < 0.5 ? 4 : time < 1 ? 2 : 4;
  const fitHeight = time < 0.5 ? 4 : 2;
  const uv = [(placed[0] * 4 - (4 - fitWidth) / 2) / fitWidth,
    1 - (placed[1] * 2 - (2 - fitHeight) / 2) / fitHeight];
  if (time >= 0.5 && time < 1 && uv.some(value => value < 0 || value > 1)) {
    return { rgba: [0, 0, 0, 255], empty: true };
  }
  const index = value => repeated
    ? Math.floor((((value % 1) + 1) % 1) * 4)
    : Math.min(3, Math.max(0, Math.floor(value * 4)));
  const texel = texels[index(uv[1])][index(uv[0])];
  const opacity = 1 - progress / 2;
  return { rgba: [...texel.slice(0, 3).map(channel => Math.round(channel * texel[3] / 255 * opacity)), 255],
    empty: texel[3] === 0 };
}

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_IMAGE_PAINT_ARTIFACTS ?? "ci-artifacts/image-paint",
  cases: sampleTimes.map(sampleTime => ({
    id: `image-paint-${sampleTime}`,
    file: "image_paint.py", source,
    factory: "createDirectImagePaintSmokeRenderer", objectCount: 1,
    duration: 4, sampleTime, playback: "live", boundaries,
    directHeldSampleTime: heldTimes.get(sampleTime),
    stopAtSourceCompletion: sampleTime === 4,
  })),
  prepareContext: disableAuthoringJspi,
  qualifyPixels({ fixture, rust, python }) {
    const time = fixture.sampleTime;
    const progress = Math.max(0, Math.min(1, time - 2));
    let checks = 0, emptyWitnesses = 0, maximumByteError = 0;
    for (const image of [rust, python]) {
      const pixelsPerUnit = image.height / 8;
      for (const u of [0.08, 0.18, 0.33, 0.43, 0.58, 0.68, 0.83, 0.93]) {
        for (const v of [0.12, 0.28, 0.46, 0.62, 0.83]) {
          const x = Math.floor(image.width / 2 + (4 * u - 2 + 0.5 * progress) * pixelsPerUnit);
          const y = Math.floor(image.height / 2 - (2 * v - 1 + 0.25 * progress) * pixelsPerUnit);
          const local = [((x + 0.5 - image.width / 2) / pixelsPerUnit - 0.5 * progress + 2) / 4,
            ((image.height / 2 - y - 0.5) / pixelsPerUnit - 0.25 * progress + 1) / 2];
          const expected = reference(time, local);
          const at = (y * image.width + x) * 4;
          const actual = [...image.data.subarray(at, at + 4)];
          const error = Math.max(...actual.map((channel, index) => Math.abs(channel - expected.rgba[index])));
          maximumByteError = Math.max(maximumByteError, error);
          assert.ok(error <= 1, `${fixture.id} (${x},${y}): ${actual} != ${expected.rgba}`);
          if (expected.empty) {
            assert.deepEqual(actual, [0, 0, 0, 255], "contain margins and hidden RGB must remain transparent on black");
            emptyWitnesses++;
          }
          checks++;
        }
      }
    }
    if (time < 2) assert.ok(emptyWitnesses > 0, `${fixture.id}: missing transparent witness`);
    return { interiorPixelComparisons: checks, emptyWitnesses, maximumByteError,
      reference: "independent physical fit, inverse placement, nearest texel, alpha and authored-time formula" };
  },
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const page = await context.newPage();
    page.setDefaultTimeout(90_000);
    try {
      await page.goto(`${baseUrl}/web/manim-raster-host.html`);
      await page.waitForFunction(() => window.noonHostRaster);
      const python = await page.evaluate(async ({ source, boundaries }) => {
        await window.noonHostRaster.load(source, 4);
        const times = [0, ...boundaries, 4];
        const final = await window.noonHostRaster.renderThrough(times.length - 1, times, { stopAtSourceCompletion: true });
        const before = await window.noonHostRaster.debugFrame();
        await new Promise(resolve => setTimeout(resolve, 100));
        const after = await window.noonHostRaster.debugFrame();
        return { final, stableFrame: JSON.stringify(before) === JSON.stringify(after),
          sourceState: window.noonHostRaster.status().sourceState };
      }, { source, boundaries });
      assert.equal(python.final.rendererBackend, expectedBackend);
      assert.equal(python.final.sourceCompleted, true);
      assert.equal(python.sourceState, "completed");
      assert.equal(python.final.time, 4);
      assert.equal(python.final.objectCount, 1);
      assert.equal(python.stableFrame, true, "completed worker source must retain its final solid-filled frame");

      await page.goto(`${baseUrl}/web/manim-raster-host.html`);
      const direct = await page.evaluate(async boundaries => {
        const wasm = await import("./pkg/noon_web.js");
        await wasm.default();
        const renderer = await wasm.createDirectImagePaintSmokeRenderer(document.querySelector("#scene").transferControlToOffscreen());
        try {
          renderer.advanceDirectRealtime(0);
          let presented = false;
          for (let attempt = 0; attempt < 60 && !presented; attempt++) {
            presented = renderer.render();
            if (!presented) await new Promise(resolve => requestAnimationFrame(resolve));
          }
          if (!presented) throw new Error("direct image paint did not present initially");
          const { sampleDirectProgramThrough } = await import("../scripts/direct-program-sample.mjs");
          const final = await sampleDirectProgramThrough(renderer, 4, boundaries);
          return { final, cleanRedraws: [renderer.render(), renderer.render()] };
        } finally {
          renderer.free();
        }
      }, boundaries);
      assert.equal(direct.final.backend, expectedBackend);
      assert.equal(direct.final.time, 4);
      assert.equal(direct.final.objectCount, 1);
      assert.equal(direct.final.cadence, "idle", "completed direct source must request no further frames");
      assert.deepEqual(direct.cleanRedraws, [false, false], "clean image-paint completion must not redraw");
      return { python, direct };
    } finally {
      await page.close();
    }
  },
});
