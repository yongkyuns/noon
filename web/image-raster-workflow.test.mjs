import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
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


test("native oracle paths remain workspace-rooted when Cargo changes the working directory", async () => {
  const workflow = await readFile(new URL("image-raster-qualification.yml", workflowDir), "utf8");
  assert.match(workflow, /NOON_IMAGE_RASTER_OUTPUT="\$GITHUB_WORKSPACE\/browser-smoke-artifacts\/images\/native\.png"/);
  assert.match(workflow, /NOON_IMAGE_MANIM_DIRECTORY: \$\{\{ github\.workspace \}\}\/browser-smoke-artifacts\/images\/manim/);
  assert.match(workflow, /NOON_IMAGE_ARTIFACTS: \$\{\{ github\.workspace \}\}\/browser-smoke-artifacts\/images/);
  assert.match(workflow, /--test image_raster_qualification -- --ignored --nocapture/);
  assert.match(workflow, /mkdir -p browser-smoke-artifacts\/images/);
});


test("image input qualification executes real worker input cases and a pinned raster oracle", async () => {
  const workflow = await readFile(new URL("image-raster-qualification.yml", workflowDir), "utf8");
  assert.match(workflow, /--test image_seek/);
  const smoke = await readFile(new URL("../scripts/image-raster-smoke.mjs", import.meta.url), "utf8");
  assert.match(smoke, /await qualifyPythonImageInputs\(/);
  const source = await readFile(new URL("../scripts/image-input-smoke.py", import.meta.url), "utf8");
  assert.match(source, /await pyodide_js\.loadPackage\("numpy"\)/);
  assert.match(source, /not view\.flags\.c_contiguous/);
  assert.match(source, /await ImageMobject\.from_url/);
  assert.match(source, /await ImageMobject\.from_blob/);
  assert.match(source, /np\.broadcast_to/);
  const reference = await readFile(new URL("../scripts/image-manim-reference.py", import.meta.url), "utf8");
  assert.match(reference, /input_camera\.capture_mobjects\(\[input_image\]\)/);
  assert.match(reference, /input-raster\.png/);
});


test("all pinned Manim oracles share the compatible Pillow constraint", async () => {
  const constraints = await readFile(
    new URL("../.github/ci/manim-reference-constraints.txt", import.meta.url), "utf8",
  );
  assert.match(constraints, /^pillow==12\.3\.0$/m);
  let installers = 0;
  for (const name of await readdir(workflowDir)) {
    if (!name.endsWith(".yml")) continue;
    const source = await readFile(new URL(name, workflowDir), "utf8");
    const installs = source.split("\n").filter(line =>
      line.includes("pip install") && /manim(?:\[typst\])?==0\.21\.0/.test(line));
    if (!installs.length) continue;
    for (const line of installs) {
      assert.match(line, /--constraint \.github\/ci\/manim-reference-constraints\.txt(?: |$)/,
        name + ": reference dependencies must not drift independently");
      installers++;
    }
    const triggers = source.split("on:\n")[1]?.split("\npermissions:")[0];
    if (triggers?.includes("    paths:\n")) {
      assert.ok(triggers.includes('      - ".github/ci/manim-reference-constraints.txt"'),
        name + ": constraint changes must select the affected oracle");
    } else {
      assert.match(triggers ?? "", /  push:\n    branches:\n      - master/);
      assert.doesNotMatch(triggers, /paths-ignore:[\s\S]*manim-reference-constraints/);
    }
  }
  assert.ok(installers > 0, "no pinned Manim oracle installers were checked");
});
