import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

const pythonPath = "web/python/_manim_family_creation.py";
const pythonSource = readFileSync(pythonPath, "utf8");
const moduleManifest = readFileSync("web/python-compat-modules.js", "utf8");
const genericRustBridge = readFileSync(
  "crates/noon-web/src/family_animation_authoring.rs",
  "utf8",
);
const writeRustBridge = readFileSync(
  "crates/noon-web/src/family_write_authoring.rs",
  "utf8",
);
const canonicalWire = readFileSync(
  "crates/noon-web/src/retained_authoring_wire_scene.rs",
  "utf8",
);

test("retained family creation-animation module is valid Python and bundled", () => {
  const result = spawnSync("python3", ["-m", "py_compile", pythonPath], {
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  assert.match(moduleManifest, /python\/_manim_family_creation\.py/);
  assert.doesNotMatch(moduleManifest, /python\/_manim_family_create\.py/);
  assert.match(pythonSource, /familyAnimationRequest/);
  assert.match(pythonSource, /familyWriteAnimationRequest/);
  assert.match(pythonSource, /bindRetainedNativeText/);
});

test("Python appends family requests without owning the scene-wide scheduler limit", () => {
  assert.doesNotMatch(
    pythonSource,
    /supports one family animation request per scene/,
  );
  assert.match(
    pythonSource,
    /Rust owns plural-plan validation/,
  );
});

test("Python does not serialize semantic family order or retained resource identity", () => {
  for (const forbidden of [
    "memberSlot(",
    "memberGeneration(",
    "glyph_id",
    "glyphIds",
    "atlas_id",
    "font_bytes",
  ]) {
    assert.equal(
      pythonSource.includes(forbidden),
      false,
      `Python family authoring must not contain ${forbidden}`,
    );
  }
  assert.match(genericRustBridge, /layout\.include_mobject/);
  assert.match(genericRustBridge, /layout\.include_retained_native_text/);
  assert.match(genericRustBridge, /FamilyAnimationRequest::new/);
});

test("Write defaults stay Rust-owned and depend on rendered retained members", () => {
  assert.doesNotMatch(pythonSource, /len\([^\n]*(?:source|text)/i);
  assert.match(writeRustBridge, /plain_text_animation_members/);
  assert.match(writeRustBridge, /write_duration\(self\.member_count/);
  assert.match(writeRustBridge, /write_lag_ratio\(self\.member_count/);
  assert.match(writeRustBridge, /FamilyAnimationMode::DrawBorderThenFill/);
  assert.match(writeRustBridge, /FamilyAnimationRequest::new/);
});

test("canonical retained normalization owns family animation transport", () => {
  assert.match(canonicalWire, /family_animations: Vec<FamilyAnimationRequest>/);
  assert.match(canonicalWire, /spec\.family_animations = wire\.family_animations/);
  assert.match(canonicalWire, /spec\.validate\(\)/);
});
