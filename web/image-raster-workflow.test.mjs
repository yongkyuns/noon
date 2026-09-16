import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const workflowDir = new URL("../.github/workflows/", import.meta.url);

test("image raster qualification builds release-capable fixtures and runs late rollback regression", async () => {
  const workflow = await readFile(new URL("image-raster-qualification.yml", workflowDir), "utf8");
  assert.match(workflow, /NOON_RENDERER_SMOKE: '1'/);
  assert.match(workflow, /--test image_late_publication_failure/);
  assert.match(workflow, /run: bash scripts\/build-web-demo\.sh/);
  assert.match(workflow, /manim==0\.21\.0/);
  assert.match(workflow, /node scripts\/image-raster-smoke\.mjs/);
  const engine = await readFile(new URL("../crates/noon-web/src/lib.rs", import.meta.url), "utf8");
  const guard = /#\[cfg\(all\(\s*feature = "renderer",\s*target_arch = "wasm32",\s*any\(debug_assertions, feature = "renderer-smoke"\)\s*\)\)\]\s*/;
  assert.match(engine, new RegExp(guard.source + "mod raster_image_smoke;"));
  assert.match(engine, new RegExp(guard.source + "pub use raster_image_smoke::\\*;"));
});
