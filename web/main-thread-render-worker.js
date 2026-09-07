export class MainThreadRenderWorker extends EventTarget {
  #closed = false;
  #controllerPromise;

  constructor() {
    super();
    this.#controllerPromise = import("./authoring-render-controller.js").then(
      (module) => {
        if (this.#closed) return null;
        return module.createAuthoringRenderController({
          postMessage: (message) => {
            if (this.#closed) return;
            queueMicrotask(() => {
              if (!this.#closed) {
                this.dispatchEvent(new MessageEvent("message", { data: message }));
              }
            });
          },
          close: () => {
            this.#closed = true;
          },
          requestAnimationFrame:
            typeof globalThis.requestAnimationFrame === "function"
              ? (callback) => globalThis.requestAnimationFrame(callback)
              : null,
          cancelAnimationFrame:
            typeof globalThis.cancelAnimationFrame === "function"
              ? (handle) => globalThis.cancelAnimationFrame(handle)
              : null,
        });
      },
      (error) => {
        this.#reportError(error);
        return null;
      },
    );
  }

  postMessage(message, _transfer = undefined) {
    if (this.#closed) return;
    queueMicrotask(() => {
      if (this.#closed) return;
      void this.#controllerPromise.then(async (controller) => {
        if (controller === null || this.#closed) return;
        try {
          await controller.dispatch(message);
        } catch (error) {
          this.#reportError(error);
        }
      });
    });
  }

  terminate() {
    if (this.#closed) return;
    this.#closed = true;
    void this.#controllerPromise.then((controller) => {
      controller?.shutdown();
    });
  }

  #reportError(error) {
    if (this.#closed) return;
    const normalized = error instanceof Error ? error : new Error(String(error));
    queueMicrotask(() => {
      if (this.#closed) return;
      this.dispatchEvent(
        new ErrorEvent("error", {
          message: normalized.message,
          error: normalized,
        }),
      );
    });
  }
}
