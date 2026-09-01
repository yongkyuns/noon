import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const pythonPath = "web/python/_manim_family_create.py";
const pythonSource = readFileSync(pythonPath, "utf8");
const moduleManifest = readFileSync("web/python-compat-modules.js", "utf8");
const rustBridge = readFileSync(
  "crates/noon-web/src/family_animation_authoring.rs",
  "utf8",
);
const canonicalWire = readFileSync(
  "crates/noon-web/src/retained_authoring_wire_scene.rs",
  "utf8",
);

test("retained family Create authoring module is valid Python and bundled", () => {
  const result = spawnSync("python3", ["-m", "py_compile", pythonPath], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(moduleManifest, /python\/_manim_family_create\.py/);
  assert.match(pythonSource, /familyAnimationRequest/);
  assert.match(pythonSource, /bindRetainedNativeText/);
});

test("Python does not serialize semantic family order or retained resource identity", () => {
  for (const forbidden of [
    "memberSlot(",
    "memberGeneration(",
    "glyph",
    "atlas",
    "font_bytes",
  ]) {
    assert.equal(
      pythonSource.includes(forbidden),
      false,
      `Python family authoring must not contain ${forbidden}`,
    );
  }
  assert.match(rustBridge, /layout\.include_mobject/);
  assert.match(rustBridge, /layout\.include_retained_native_text/);
  assert.match(rustBridge, /FamilyAnimationRequest::new/);
});

test("canonical retained normalization owns family animation transport", () => {
  assert.match(canonicalWire, /family_animations: Vec<FamilyAnimationRequest>/);
  assert.match(canonicalWire, /spec\.family_animations = wire\.family_animations/);
  assert.match(canonicalWire, /spec\.validate\(\)/);
});
