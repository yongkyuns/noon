// Review-only canvas layout. Runtime resize and seek remain owned by the host;
// never assign the bitmap dimensions, move/replace the canvas, or resample PNGs.
import assert from "node:assert/strict";

function validSize(size) {
  assert.ok(size && Number.isSafeInteger(size.width) && size.width > 0 &&
    Number.isSafeInteger(size.height) && size.height > 0, "invalid replay viewport");
}

export function assertReplayViewport(observed, expected) {
  validSize(expected);
  assert.equal(observed.deviceScaleFactor, 1, "replay requires one device pixel per CSS pixel");
  assert.deepEqual(observed.bounds, { x: 0, y: 0, ...expected }, "replay canvas is not integer-aligned");
  assert.deepEqual(observed.bitmap, expected, "renderer has not resized its backing bitmap");
}

export async function layoutReplayViewport(canvas, size) {
  validSize(size);
  await canvas.evaluate((element, { width, height }) => {
    const styles = {
      position: "fixed", inset: "auto", left: "0px", top: "0px",
      width: `${width}px`, height: `${height}px`,
      "min-width": `${width}px`, "max-width": `${width}px`,
      "min-height": `${height}px`, "max-height": `${height}px`,
      display: "block", "box-sizing": "content-box", margin: "0px", padding: "0px",
      border: "0px", "border-radius": "0px", "box-shadow": "none", outline: "none",
      transform: "none", filter: "none", opacity: "1", background: "#000",
      "z-index": "2147483647", "pointer-events": "none",
    };
    for (const [property, value] of Object.entries(styles)) element.style.setProperty(property, value, "important");
    // Give the existing ResizeObserver a layout opportunity. This is NOT a
    // presentation acknowledgement: the caller must still await a normal seek.
    return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  }, size);
}

export async function replayViewport(canvas, expected) {
  const observed = await canvas.evaluate(element => {
    const { x, y, width, height } = element.getBoundingClientRect();
    return { bounds: { x, y, width, height },
      bitmap: { width: element.width, height: element.height }, deviceScaleFactor: window.devicePixelRatio };
  });
  assertReplayViewport(observed, expected);
  return observed;
}

// A real browser regression, run before live review using its already-installed
// PNG decoder. An opaque backing bitmap is an independent layout/pixel oracle;
// this fixture does not stand in for any Noon scene or runtime qualification.
export async function qualifyReplayViewport(browser, decodePng, galleryDocument) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const size = { width: 960, height: 540 };
  const cases = [];
  try {
    assert.equal(typeof galleryDocument, "string", "gallery shell fixture is missing");
    const fixtures = [0.25, 0.5, 0.75].map(offset => ({ name: `fractional origin ${offset}`, html: `<style>
        * { box-sizing: border-box; }
        body { margin: 0; background: magenta; }
        canvas { position: absolute; left: ${offset}px; top: ${offset}px;
          width: 960px; height: 540px; border: 1px solid red; border-radius: 13px;
          box-shadow: inset 0 0 0 1px white; background: black; }
        </style><canvas id="scene" width="960" height="540"></canvas>` }));
    // Exercise the real gallery's ancestors and decoration without running its
    // authoring/bootstrap scripts. No Noon source is executed by this fixture.
    fixtures.push({ name: "current gallery shell", html: galleryDocument.replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "") });
    for (const fixture of fixtures) {
      await page.setContent(fixture.html);
      const canvas = page.locator("#scene");
      const bitmapUrl = await canvas.evaluate((element, { width, height }) => {
        element.width = width; element.height = height; // fixture bitmap only
        const context = element.getContext("2d");
        const pixels = context.createImageData(element.width, element.height);
        for (let i = 0; i < pixels.data.length; i += 4) {
          const x = (i / 4) % element.width, y = Math.floor(i / 4 / element.width);
          pixels.data.set([x % 251, y % 241, (x + y) % 239, 255], i);
        }
        context.putImageData(pixels, 0, 0);
        return element.toDataURL();
      }, size);
      const reference = decodePng(Buffer.from(bitmapUrl.split(",")[1], "base64"));
      const before = decodePng(await canvas.screenshot());
      assert.ok(before.width !== size.width || before.height !== size.height || !before.data.equals(reference.data),
        "fixture no longer reproduces the decorated/fractional-origin failure");
      await layoutReplayViewport(canvas, size);
      const observed = await replayViewport(canvas, size);
      const after = decodePng(await canvas.screenshot());
      assert.deepEqual({ width: after.width, height: after.height }, size);
      assert.ok(after.data.equals(reference.data), "aligned screenshot differs from original backing pixels");
      cases.push({ name: fixture.name, before: { width: before.width, height: before.height }, ...observed, exactPixels: true });
    }
    return { scope: "browser layout fixture, not Noon runtime execution", cases };
  } finally {
    await context.close();
  }
}
