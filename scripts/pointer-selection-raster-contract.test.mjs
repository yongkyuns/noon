import test from "node:test";
import assert from "node:assert/strict";
import { VIEW, SHAPES, selectionFixtureSource, shapeSurfaceCenter, assertExactPixels,
  assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";

const image = (view = VIEW) => ({ width: view.width, height: view.height,
  data: Buffer.alloc(view.width * view.height * 4, 0xff) });
// Synthetic mutations test the oracle only; they are not renderer evidence.
function fill(shape, view = VIEW) {
  const result = image(view);
  const center = shapeSurfaceCenter(shape, view), scale = view.height / view.cameraHeight;
  for (let y = 0; y < result.height; y++) for (let x = 0; x < result.width; x++) {
    const dx = (x + 0.5 - center.x) / scale, dy = -(y + 0.5 - center.y) / scale;
    const u = (dx * Math.cos(shape.rotation) + dy * Math.sin(shape.rotation)) / shape.scaleX;
    const v = (-dx * Math.sin(shape.rotation) + dy * Math.cos(shape.rotation)) / shape.scaleY;
    if (shape.kind === "circle" ? u * u + v * v <= shape.radius * shape.radius :
      Math.abs(u) <= shape.width / 2 && Math.abs(v) <= shape.height / 2) result.data[(y * result.width + x) * 4] = 0;
  }
  return result;
}

test("fixture source and click points share the same explicit geometry", () => {
  const source = selectionFixtureSource();
  assert.match(source, /class PointerSelectionFixture\(Scene\)/);
  assert.doesNotMatch(source, /self\.(play|wait)|add_updater|callback/);
  assert.deepEqual(shapeSurfaceCenter(SHAPES[0]), { x: 248, y: 171 });
  assert.deepEqual(shapeSurfaceCenter(SHAPES[1]), { x: 387.5, y: 189 });
});
for (const shape of SHAPES) test(`${shape.kind}: accept the transformed interior, reject absent/wrong/partial tint`, () => {
  const before = image(), after = fill(shape);
  assert.ok(assertSelectionPixels(before, after, shape).changed > 100);
  assert.throws(() => assertSelectionPixels(before, before, shape), /interior/);
  assert.throws(() => assertSelectionPixels(before, fill(SHAPES.find(value => value !== shape)), shape), /unrelated/);
  const center = shapeSurfaceCenter(shape), offset = (Math.floor(center.y) * VIEW.width + Math.floor(center.x)) * 4;
  after.data[offset] = before.data[offset];
  assert.throws(() => assertSelectionPixels(before, after, shape), /interior/);
});
test("changed pixels outside the fill are not accepted as bounding-box selection", () => {
  const shape = SHAPES[0], after = fill(shape);
  after.data[0] = 0;
  assert.throws(() => assertSelectionPixels(image(), after, shape), /unrelated pixel/);
});
test("clear and redraw checks reject even a one-channel one-pixel difference", () => {
  const before = image(), after = image();
  assertExactPixels(before, after, "clear");
  after.data[7] = 0;
  assert.throws(() => assertExactPixels(before, after, "clear"), /identical/);
});
test("invalid dimensions and incomplete decoded captures fail closed", () => {
  const before = image();
  assert.throws(() => assertSelectionPixels(before, { ...image(), width: 0 }, SHAPES[0]));
  assert.throws(() => assertExactPixels(before, { ...image(), data: Buffer.alloc(3) }, "capture"), /RGBA/);
});

test("inspection oracle follows zoomed camera geometry without demanding opaque yellow", () => {
  const view = { width: 800, height: 400, cameraHeight: 4, centerX: 0.5, centerY: 0 };
  const shape = { kind: "circle", radius: 0.4, x: 2, y: 0, scaleX: 1, scaleY: 1, rotation: 0 };
  const before = image(view), after = fill(shape, view);
  // A translucent overlay changes channels, but need not exceed an opaque-color threshold.
  for (let i = 0; i < after.data.length; i += 4) if (after.data[i] === 0) after.data[i] = 147;
  assert.ok(assertSelectionPixels(before, after, shape, view).changed > 100);
  assert.throws(() => assertSelectionPixels(before, after, shape, { ...view, cameraHeight: 8, centerX: 0 }), /unrelated|interior/);
  assert.throws(() => assertSelectionPixels(before, before, shape, view), /interior/);
  after.data[0] = 0;
  assert.throws(() => assertSelectionPixels(before, after, shape, view), /unrelated/);
});

test("inspection oracle rejects an invalid expected camera before scanning pixels", () => {
  for (const cameraHeight of [0, -1, NaN, Infinity]) {
    assert.throws(() => assertSelectionPixels(image(), image(), SHAPES[0], { ...VIEW, cameraHeight }), /invalid expected/);
  }
});
