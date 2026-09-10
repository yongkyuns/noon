import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const semanticHandlesSource = readFileSync(
  new URL("./python/_manim_semantic_handles.py", import.meta.url),
  "utf8",
);
const facadeSource = readFileSync(new URL("./python/noon.py", import.meta.url), "utf8");
const callbackSource = readFileSync(new URL("./python/_manim_updaters.py", import.meta.url), "utf8");
const rustHandleSource = readFileSync(
  new URL("../crates/noon/src/semantic_mobject.rs", import.meta.url),
  "utf8",
);

function functionBody(source, name) {
  const start = source.indexOf(`def ${name}(`);
  assert.notEqual(start, -1, `missing Python function ${name}`);
  const end = source.indexOf("\ndef ", start);
  assert.notEqual(end, -1, `missing Python function boundary after ${name}`);
  return source.slice(start, end);
}

test("detached Mobject layout queries stay owned by the shared semantic handle", () => {
  const layoutCenter = functionBody(semanticHandlesSource, "_layout_center");
  assert.match(layoutCenter, /_handle_for\(value\)/);
  assert.match(layoutCenter, /handle\.centerX/);
  assert.match(layoutCenter, /handle\.centerY/);

  const getCenter = functionBody(semanticHandlesSource, "_get_center");
  assert.match(getCenter, /_handle_for\(self\)/);
  assert.match(getCenter, /return _layout_center\(self\)/);

  const width = functionBody(semanticHandlesSource, "_width");
  assert.match(width, /handle\.width/);

  const height = functionBody(semanticHandlesSource, "_height");
  assert.match(height, /handle\.height/);

  assert.match(facadeSource, /return _callback_operations\(\)\._canonical_get_center\(self\)/);
  assert.match(callbackSource, /return _base\._semantic_operations\(\)\._get_center\(self\)/);
  assert.doesNotMatch(callbackSource, /def install\(|_ORIGINAL_/);

  for (const property of ["width", "height"]) {
    assert.ok(facadeSource.includes(`return _semantic_operations()._${property}(self)`));
    assert.ok(facadeSource.includes(`_semantic_operations()._set_${property}_property(self, value)`));
  }
  assert.doesNotMatch(semanticHandlesSource, /def install\(|_ORIGINAL_|^_base\.Mobject\.[a-z_]+ =/m);

});

test("Rust semantic handle remains the layout-query source of truth", () => {
  assert.match(rustHandleSource, /pub fn layout_bounds\(&self\) -> Result<Option<Bounds2D64>, AuthoringError>/);
  assert.match(rustHandleSource, /pub fn center\(&self\) -> Result<\(f64, f64\), AuthoringError>/);
  assert.match(rustHandleSource, /pub fn width\(&self\) -> Result<f64, AuthoringError>/);
  assert.match(rustHandleSource, /pub fn height\(&self\) -> Result<f64, AuthoringError>/);
  assert.match(
    rustHandleSource,
    /pub fn critical_point\([\s\S]*?\) -> Result<\(f64, f64\), AuthoringError>/,
  );

  const centerStart = rustHandleSource.indexOf("pub fn center(&self)");
  const widthStart = rustHandleSource.indexOf("pub fn width(&self)", centerStart);
  assert.ok(centerStart >= 0 && widthStart > centerStart);
  const centerBody = rustHandleSource.slice(centerStart, widthStart);
  assert.match(centerBody, /self\.layout_bounds\(\)/);

  const widthEnd = rustHandleSource.indexOf("pub fn height(&self)", widthStart);
  const widthBody = rustHandleSource.slice(widthStart, widthEnd);
  assert.match(widthBody, /self\.layout_bounds\(\)/);

  const heightEnd = rustHandleSource.indexOf("pub fn critical_point", widthEnd);
  const heightBody = rustHandleSource.slice(widthEnd, heightEnd);
  assert.match(heightBody, /self\.layout_bounds\(\)/);
});
