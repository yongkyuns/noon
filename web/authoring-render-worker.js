import { createAuthoringRenderController } from "./authoring-render-controller.js";

const controller = createAuthoringRenderController({
  postMessage(message, transfer = []) {
    self.postMessage(message, transfer);
  },
  close() {
    self.close();
  },
  requestAnimationFrame:
    typeof self.requestAnimationFrame === "function"
      ? (callback) => self.requestAnimationFrame(callback)
      : null,
  cancelAnimationFrame:
    typeof self.cancelAnimationFrame === "function"
      ? (handle) => self.cancelAnimationFrame(handle)
      : null,
});

self.addEventListener("message", (event) => {
  void controller.dispatch(event.data);
});
