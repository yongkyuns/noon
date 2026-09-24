// Independent pixel assertions for the selection qualification fixture. These
// inspect captured output; they never participate in production picking/rendering.
import assert from "node:assert/strict";

export const VIEW = Object.freeze({ width: 640, height: 360, cameraHeight: 8 });
export const SHAPES = Object.freeze([
  Object.freeze({ kind: "circle", radius: 0.9, x: -1.6, y: 0.2,
    scaleX: -1.3, scaleY: 0.65, rotation: 0.45, color: "BLUE" }),
  Object.freeze({ kind: "rectangle", width: 1.7, height: 1.2, x: 1.5, y: -0.2,
    scaleX: 1, scaleY: 1, rotation: -0.5, color: "GREEN" }),
]);

export function selectionFixtureSource() {
  const shapes = SHAPES.map((shape, index) => {
    const constructor = shape.kind === "circle" ? `Circle(radius=${shape.radius}` :
      `Rectangle(width=${shape.width}, height=${shape.height}`;
    return `        shape${index} = ${constructor}, color=${shape.color}, fill_opacity=1, stroke_width=0)\n` +
      `        shape${index}.stretch(${shape.scaleX}, 0).stretch(${shape.scaleY}, 1).rotate(${shape.rotation}).shift([${shape.x}, ${shape.y}, 0])`;
  });
  return ["from noon import *", "", "class PointerSelectionFixture(Scene):", "    def construct(self):",
    ...shapes, '        label = Text("Click a filled shape; background clears", font_size=24).shift(2.8 * UP)',
    "        self.add(shape0, shape1, label)", ""].join("\n");
}

export function shapeSurfaceCenter(shape, view = VIEW) {
  const pixels = view.height / view.cameraHeight;
  return { x: view.width / 2 + (shape.x - (view.centerX ?? 0)) * pixels,
    y: view.height / 2 - (shape.y - (view.centerY ?? 0)) * pixels };
}

function requireImage(image, view = VIEW) {
  assert.ok(Number.isSafeInteger(view.width) && view.width > 0 &&
    Number.isSafeInteger(view.height) && view.height > 0 &&
    Number.isFinite(view.cameraHeight) && view.cameraHeight > 0 &&
    Number.isFinite(view.centerX ?? 0) && Number.isFinite(view.centerY ?? 0), "invalid expected camera view");
  assert.ok(image && Number.isSafeInteger(image.width) && Number.isSafeInteger(image.height), "invalid image dimensions");
  assert.equal(image.width, view.width); assert.equal(image.height, view.height);
  assert.equal(image.data?.length, image.width * image.height * 4, "expected decoded RGBA pixels");
}

function localPoint(shape, x, y, view) {
  const units = view.cameraHeight / view.height;
  const worldX = (x + 0.5 - view.width / 2) * units + (view.centerX ?? 0) - shape.x;
  const worldY = (view.height / 2 - y - 0.5) * units + (view.centerY ?? 0) - shape.y;
  const cos = Math.cos(shape.rotation), sin = Math.sin(shape.rotation);
  return { x: (cos * worldX + sin * worldY) / shape.scaleX,
    y: (-sin * worldX + cos * worldY) / shape.scaleY };
}

function inside(shape, x, y, marginPixels, view) {
  const point = localPoint(shape, x, y, view);
  const margin = marginPixels * view.cameraHeight / view.height;
  if (shape.kind === "circle") {
    const radius = shape.radius + margin / Math.min(Math.abs(shape.scaleX), Math.abs(shape.scaleY));
    return radius > 0 && Math.hypot(point.x, point.y) <= radius;
  }
  return Math.abs(point.x) <= shape.width / 2 + margin / Math.abs(shape.scaleX) &&
    Math.abs(point.y) <= shape.height / 2 + margin / Math.abs(shape.scaleY);
}

export function assertExactPixels(actual, expected, label) {
  requireImage(actual); requireImage(expected);
  const firstDifference = actual.data.findIndex((value, index) => value !== expected.data[index]);
  assert.equal(firstDifference, -1, `${label}: RGBA pixels must be identical`);
}

export function assertSelectionPixels(before, after, shape, view = VIEW) {
  requireImage(before, view); requireImage(after, view);
  let changed = 0, interior = 0, changedInterior = 0;
  for (let y = 0; y < before.height; y++) {
    for (let x = 0; x < before.width; x++) {
      const offset = (y * before.width + x) * 4;
      const differs = [0, 1, 2, 3].some(channel => before.data[offset + channel] !== after.data[offset + channel]);
      if (differs) {
        changed++;
        assert.ok(inside(shape, x, y, 2, view), `selection changed unrelated pixel (${x}, ${y})`);
      }
      if (inside(shape, x, y, -2, view)) {
        interior++;
        if (differs) changedInterior++;
      }
    }
  }
  assert.ok(interior > 100, "fixture must cover a meaningful interior");
  assert.equal(changedInterior, interior, "all definite fill-interior pixels must visibly change");
  assert.ok(changed > 100, "selection must actually change captured pixels");
  return { changed, interior, changedInterior, edgeTolerancePixels: 2 };
}
