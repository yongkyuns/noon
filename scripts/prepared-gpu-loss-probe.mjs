// Fault injection only: observe native GPU owners and destroy/lose them. This
// probe never supplies scene state, authored time, completion or display receipts.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";

export async function installPreparedGpuLossProbe(context, evidence) {
  await context.route(url => new URL(url).pathname.endsWith("/web/execution-render-worker.js"), async route => {
    const response = await route.fetch(), original = await response.text();
    const body = `import { installCaptureState } from "../scripts/webgpu-device-capture.mjs";
installCaptureState();
globalThis.noonLossContexts = [];
const originalContext = OffscreenCanvas.prototype.getContext;
OffscreenCanvas.prototype.getContext = function(kind, ...args) {
  const context = originalContext.call(this, kind, ...args);
  if (kind === "webgl2" && context && !noonLossContexts.some(item => item.context === context)) {
    noonLossContexts.push({ canvas: this, context });
  }
  return context;
};
` + original;
    const hash = value => createHash("sha256").update(value).digest("hex");
    evidence.gpuLossProbe = { scope: "native device/context capture and real loss injection; no semantic or timing input",
      originalWorkerSha256: hash(original), executedWorkerSha256: hash(body) };
    const headers = { ...response.headers() }; delete headers["content-length"]; delete headers["content-encoding"];
    await route.fulfill({ response, headers, body });
  });
}

export async function losePreparedRenderer(page, backend) {
  const worker = page.workers().findLast(worker => new URL(worker.url()).pathname.endsWith("/web/execution-render-worker.js"));
  assert.ok(worker, "the gallery must own a real render worker");
  const dimensions = await page.locator("#scene").evaluate(canvas => ({ width: canvas.width, height: canvas.height }));
  if (backend === "webgpu") {
    return worker.evaluate(async () => {
      const capture = globalThis.__noonWebGpuDeviceCapture;
      const index = capture?.configuredCanvasIds.findLastIndex(ids => ids.length > 0) ?? -1;
      if (index < 0) throw new Error(`no configured renderer GPUDevice: ${capture?.patchError}`);
      const device = capture.devices[index];
      device.destroy();
      const lost = await device.lost;
      return { backend: "webgpu", deviceIndex: index, configuredCanvasIds: capture.configuredCanvasIds[index],
        reason: String(lost.reason), message: String(lost.message) };
    });
  }
  await worker.evaluate(dimensions => {
    const item = globalThis.noonLossContexts.findLast(item => item.canvas.width === dimensions.width && item.canvas.height === dimensions.height);
    const extension = item?.context.getExtension("WEBGL_lose_context");
    if (!extension) throw new Error("the renderer WebGL context cannot be lost/restored");
    globalThis.noonContextLoss = { item, extension, lost: false, restored: false };
    item.canvas.addEventListener("webglcontextlost", event => {
      event.preventDefault(); noonContextLoss.lost = true;
    }, { once: true });
    item.canvas.addEventListener("webglcontextrestored", () => { noonContextLoss.restored = true; }, { once: true });
    extension.loseContext();
  }, dimensions);
  // Restore in a later browser task, after loss event dispatch has completed.
  // Restoring from a listener's microtask can precede browser restore admission.
  await waitForContext(worker, "lost");
  await worker.evaluate(() => noonContextLoss.extension.restoreContext());
  await waitForContext(worker, "restored");
  return worker.evaluate(() => ({ backend: "webgl", width: noonContextLoss.item.canvas.width,
    height: noonContextLoss.item.canvas.height, lost: noonContextLoss.lost, restored: noonContextLoss.restored }));
}

async function waitForContext(worker, key) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (await worker.evaluate(key => globalThis.noonContextLoss[key], key)) return;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  throw new Error(`render worker did not observe real WebGL context ${key}`);
}
