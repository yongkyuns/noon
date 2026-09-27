import init, { createDirectSingleTypstCanvasRenderer } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
if (!(canvas instanceof HTMLCanvasElement)) {
  throw new Error("direct Typst raster canvas is missing");
}

const initialized = init();
let rendered = false;

window.noonDirectTypstRaster = {
  ready: () => initialized,
  async render({ source, math, fontSize, width, height }) {
    if (rendered) {
      throw new Error("direct Typst raster host supports one fixture per page");
    }
    rendered = true;
    await initialized;
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const offscreen = canvas.transferControlToOffscreen();
    const renderer = await createDirectSingleTypstCanvasRenderer(
      offscreen,
      source,
      Boolean(math),
      Number(fontSize),
    );
    renderer.resize(width, height);
    renderer.render();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return {
      objectCount: renderer.objectCount(),
      drawCalls: renderer.lastDrawCalls(),
      instancesDrawn: renderer.lastInstancesDrawn(),
      bytesUploaded: renderer.lastBytesUploaded(),
    };
  },
};
