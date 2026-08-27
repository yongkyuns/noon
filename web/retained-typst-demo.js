import init, { RetainedTypstCanvasRenderer } from "./pkg/noon_web.js";

const canvas = document.querySelector("#scene");
const status = document.querySelector("#status");

if (!(canvas instanceof HTMLCanvasElement) || !(status instanceof HTMLOutputElement)) {
  throw new Error("retained Typst demo DOM is incomplete");
}

await init();

const offscreen = canvas.transferControlToOffscreen();
const renderer = await RetainedTypstCanvasRenderer.create(
  offscreen,
  "*Hello* from _Noon Typst!_",
  "frac(x^2 + y^2, 2) = sum_(k=1)^n k",
);

function resize() {
  const rect = canvas.getBoundingClientRect();
  const dpr = Math.max(1, globalThis.devicePixelRatio || 1);
  const width = Math.max(1, Math.round(rect.width * dpr));
  const height = Math.max(1, Math.round(rect.height * dpr));
  renderer.resize(width, height);
  renderer.render();
  status.value = `${renderer.objectCount()} objects · ${renderer.lastDrawCalls()} draws · ${renderer.lastBytesUploaded()} B upload`;
}

new ResizeObserver(resize).observe(canvas);
resize();
