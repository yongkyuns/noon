import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const semanticHandlesSource = readFileSync(
  new URL("./python/_manim_semantic_handles.py", import.meta.url),
  "utf8",
);
const rustHandleSource = readFileSync(
  new URL("../crates/noon/src/semantic_mobject.rs", import.meta.url),
  "utf8",
);

function functionBody(source, name, nextName) {
  const start = source.indexOf(`def ${name}(`);
  assert.notEqual(start, -1, `missing Python function ${name}`);
  const end = source.indexOf(`\ndef ${nextName}(`, start);
  assert.notEqual(end, -1, `missing Python function boundary after ${name}`);
  return source.slice(start, end);
}

test("detached Mobject layout queries stay owned by the shared semantic handle", () => {
  const layoutCenter = functionBody(semanticHandlesSource, "_layout_center", "_init");
  assert.match(layoutCenter, /_handle_for\(value\)/);
  assert.match(layoutCenter, /handle\.centerX/);
  assert.match(layoutCenter, /handle\.centerY/);

  const getCenter = functionBody(semanticHandlesSource, "_get_center", "_width");
  assert.match(getCenter, /_handle_for\(self\)/);
  assert.match(getCenter, /return _layout_center\(self\)/);

  const width = functionBody(semanticHandlesSource, "_width", "_height");
  assert.match(width, /handle\.width/);

  const height = functionBody(semanticHandlesSource, "_height", "_set_width_property");
  assert.match(height, /handle\.height/);

  assert.match(
    semanticHandlesSource,
    /_base\.Mobject\.get_center = _get_center/,
    "install() must keep get_center on the semantic-handle adapter",
  );
  assert.match(
    semanticHandlesSource,
    /_base\.Mobject\.width = property\(_width, _set_width_property\)/,
    "install() must keep width on the semantic-handle adapter",
  );
  assert.match(
    semanticHandlesSource,
    /_base\.Mobject\.height = property\(_height, _set_height_property\)/,
    "install() must keep height on the semantic-handle adapter",
  );
});

test("Rust semantic handle remains the layout-query source of truth", () => {
  assert.match(rustHandleSource, /pub fn layout_bounds\(&self\) -> Result<Option<Bounds2D64>, String>/);
  assert.match(rustHandleSource, /pub fn center\(&self\) -> Result<\(f64, f64\), String>/);
  assert.match(rustHandleSource, /pub fn width\(&self\) -> Result<f64, String>/);
  assert.match(rustHandleSource, /pub fn height\(&self\) -> Result<f64, String>/);
  assert.match(
    rustHandleSource,
    /pub fn critical_point\([\s\S]*?\) -> Result<\(f64, f64\), String>/,
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
