import { createAuthoringRenderController } from "./authoring-render-controller.js";
import { createWorkerFrameSource } from "./render-frame-source.js";

let controller = null;
let documentFrames = null;

self.addEventListener("message", (event) => {
  if (documentFrames?.receive(event.data)) return;
  if (controller === null) {
    // A document may supply its refresh opportunity to an OffscreenCanvas
    // owner. Standalone worker hosts retain their native RAF/timer capability.
    if (event.data?.type === "prepare" && event.data.documentAnimationFrames === true) {
      documentFrames = createWorkerFrameSource({
        send: (message) => self.postMessage(message),
        now: () => performance.now(),
      });
    }
    const nativeFrames = typeof self.requestAnimationFrame === "function";
    controller = createAuthoringRenderController({
      postMessage: (message) => self.postMessage(message),
      close() {
        try { documentFrames?.close(); } finally { self.close(); }
      },
      animationFrameSource: documentFrames !== null ? "document" : nativeFrames ? "worker" : "timer",
      requestAnimationFrame: documentFrames?.request ?? (nativeFrames
        ? (callback) => self.requestAnimationFrame(callback) : null),
      cancelAnimationFrame: documentFrames?.cancel ?? (nativeFrames
        ? (handle) => self.cancelAnimationFrame(handle) : null),
    });
  }
  void controller.dispatch(event.data);
});
