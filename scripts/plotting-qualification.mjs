import assert from "node:assert/strict";
import { qualifyPairedAuthoring } from "./paired-authoring-qualification.mjs";

await qualifyPairedAuthoring({
  artifactDirectory: process.env.NOON_PLOTTING_ARTIFACTS ?? "plotting-artifacts",
  cases: [
    { id: "coordinates", file: "coordinate_plotting_static.py", factory: "createCoordinatePlottingRenderer", objectCount: 17 },
    { id: "polar-plane", file: "polar_plane.py", factory: "createPolarPlaneRenderer", objectCount: 34 },
    { id: "number-plane", file: "number_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
    { id: "complex-plane", file: "complex_plane.py", factory: "createNumberPlaneRenderer", objectCount: 24 },
    { id: "area", file: "area_helpers.py", factory: "createAreaHelpersRenderer", objectCount: 26 },
    { id: "implicit", file: "implicit_plotting.py", factory: "createImplicitPlottingRenderer", objectCount: 16 },
  ],
  async qualifyLifecycle(context, baseUrl, expectedBackend) {
    const page = await context.newPage();
    try {
      page.setDefaultTimeout(90_000);
      await page.goto(`${baseUrl}/web/manim-compat-smoke.html`);
      await page.waitForFunction(() => window.noonManimCompat);
      const lifecycle = await page.evaluate(async () => (await window.noonManimCompat.ready()).plotting);
      assert.equal(lifecycle.backend, expectedBackend);
      assert.equal(lifecycle.objectCount, 14);
      assert.ok(Math.abs(lifecycle.duration - 0.8) < 1e-6);
      assert.ok(lifecycle.presentedFrames > 0);
      return lifecycle;
    } finally {
      await page.close();
    }
  },
});
