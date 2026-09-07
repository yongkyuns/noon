import {
  configureAuthoringRenderHost,
  dispatchAuthoringRenderMessage,
  resetAuthoringRenderController,
} from "./authoring-render-controller.js";

resetAuthoringRenderController();
configureAuthoringRenderHost({
  postMessage(message) {
    self.postMessage(message);
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
  void dispatchAuthoringRenderMessage(event.data);
});
