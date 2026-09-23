"""Staging-only repair of the raster oracle; production runtime is unchanged."""
from pathlib import Path


def edit(path, pairs):
    p = Path(path)
    s = p.read_text()
    for old, new in pairs:
        assert s.count(old) == 1, (path, old, s.count(old))
        s = s.replace(old, new)
    p.write_text(s)


edit('scripts/pointer-selection-raster-contract.mjs', [
    ('export function shapeSurfaceCenter(shape) {\n  const pixels = VIEW.height / VIEW.cameraHeight;\n  return { x: VIEW.width / 2 + shape.x * pixels, y: VIEW.height / 2 - shape.y * pixels };\n}', '''export function shapeSurfaceCenter(shape, view = VIEW) {
  const pixels = view.height / view.cameraHeight;
  return { x: view.width / 2 + (shape.x - (view.centerX ?? 0)) * pixels,
    y: view.height / 2 - (shape.y - (view.centerY ?? 0)) * pixels };
}'''),
    ('function requireImage(image) {', '''function requireImage(image, view = VIEW) {
  assert.ok(Number.isSafeInteger(view.width) && view.width > 0 &&
    Number.isSafeInteger(view.height) && view.height > 0 &&
    Number.isFinite(view.cameraHeight) && view.cameraHeight > 0 &&
    Number.isFinite(view.centerX ?? 0) && Number.isFinite(view.centerY ?? 0), "invalid expected camera view");'''),
    ('assert.equal(image.width, VIEW.width); assert.equal(image.height, VIEW.height);', 'assert.equal(image.width, view.width); assert.equal(image.height, view.height);'),
    ('function localPoint(shape, x, y) {\n  const units = VIEW.cameraHeight / VIEW.height;\n  const worldX = (x + 0.5 - VIEW.width / 2) * units - shape.x;\n  const worldY = (VIEW.height / 2 - y - 0.5) * units - shape.y;', '''function localPoint(shape, x, y, view) {
  const units = view.cameraHeight / view.height;
  const worldX = (x + 0.5 - view.width / 2) * units + (view.centerX ?? 0) - shape.x;
  const worldY = (view.height / 2 - y - 0.5) * units + (view.centerY ?? 0) - shape.y;'''),
    ('function inside(shape, x, y, marginPixels) {\n  const point = localPoint(shape, x, y);\n  const margin = marginPixels * VIEW.cameraHeight / VIEW.height;', '''function inside(shape, x, y, marginPixels, view) {
  const point = localPoint(shape, x, y, view);
  const margin = marginPixels * view.cameraHeight / view.height;'''),
    ('export function assertSelectionPixels(before, after, shape) {\n  requireImage(before); requireImage(after);', 'export function assertSelectionPixels(before, after, shape, view = VIEW) {\n  requireImage(before, view); requireImage(after, view);'),
    ('inside(shape, x, y, 2)', 'inside(shape, x, y, 2, view)'),
    ('inside(shape, x, y, -2)', 'inside(shape, x, y, -2, view)'),
])
edit('scripts/pointer-selection-raster-contract.test.mjs', [
    ('const image = () => ({ width: VIEW.width, height: VIEW.height,\n  data: Buffer.alloc(VIEW.width * VIEW.height * 4, 0xff) });', 'const image = (view = VIEW) => ({ width: view.width, height: view.height,\n  data: Buffer.alloc(view.width * view.height * 4, 0xff) });'),
    ('function fill(shape) {\n  const result = image();\n  const center = shapeSurfaceCenter(shape), scale = VIEW.height / VIEW.cameraHeight;', 'function fill(shape, view = VIEW) {\n  const result = image(view);\n  const center = shapeSurfaceCenter(shape, view), scale = view.height / view.cameraHeight;'),
])
p = Path('scripts/pointer-selection-raster-contract.test.mjs')
p.write_text(p.read_text() + '''
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
''')
edit('scripts/direct-inspection-qualification.mjs', [
    ('import { browserArgs } from "./manim-raster-support.mjs";', 'import { browserArgs } from "./manim-raster-support.mjs";\nimport { VIEW, SHAPES, assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";'),
    ('          const selected = await shot("picked-after-zoom"), i = 4 * (Math.floor(z.y) * selected.width + Math.floor(z.x));\n          assert.ok(selected.data[i] > 200 && selected.data[i + 1] > 150, "selected fill at zoomed hit point");', '''          const selected = await shot("picked-after-zoom");
          // The shared overlay is translucent. Qualify the entire transformed
          // fill and untouched exterior, not an invented opaque-yellow value.
          entry.selectionPixels = assertSelectionPixels(zoomed, selected, SHAPES[0], {
            ...VIEW, cameraHeight: VIEW.cameraHeight / 2,
            centerX: (340 - VIEW.width / 2) * VIEW.cameraHeight / VIEW.height / 2,
            centerY: (VIEW.height / 2 - 170) * VIEW.cameraHeight / VIEW.height / 2,
          });'''),
])
if Path('scripts/worker-inspection-qualification.mjs').exists():
    edit('scripts/worker-inspection-qualification.mjs', [
        ('import { browserArgs } from "./manim-raster-support.mjs";', 'import { browserArgs } from "./manim-raster-support.mjs";\nimport { assertSelectionPixels } from "./pointer-selection-raster-contract.mjs";'),
        ('        const selected = await image("picked"), i = 4 * (Math.floor(z.y) * selected.width + Math.floor(z.x));\n        assert.ok(selected.data[i] > 200 && selected.data[i + 1] > 150, "precise picking must follow zoomed rendering");', '''        const selected = await image("picked");
        entry.selectionPixels = assertSelectionPixels(zoomed, selected, {
          kind: "circle", radius: 0.4, x: 2, y: 0, scaleX: 1, scaleY: 1, rotation: 0,
        }, { width: 800, height: 400, cameraHeight: 4, centerX: 0.5, centerY: 0 });'''),
    ])
