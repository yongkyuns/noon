import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

const probeSha256 = createHash("sha256").update(await readFile(new URL(import.meta.url))).digest("hex");

/** Temporary browser-side diagnostics for Firefox audio and worker failures. */
export async function installPlaybackDiagnostics(context) {
  await context.addInitScript(() => {
    const limit = 96;
    const events = [];
    const contexts = new WeakMap();
    let nextContext = 0, nextWorker = 0, nextEvent = 0, omitted = 0;
    const marker = "[NOON_PLAYBACK_DIAG]";
    const messageTypes = new Set(["ready", "run", "result", "error", "attach_semantic_execution",
      "release_semantic_execution", "cancel_semantic_continuation"]);
    const scalar = value => {
      if (value == null || ["string", "number", "boolean"].includes(typeof value)) return value;
      return String(value).slice(0, 300);
    };
    const record = event => {
      const item = { at: performance.now(), epochMs: Date.now(), timeOrigin: performance.timeOrigin,
        sequence: ++nextEvent, ...event };
      if (events.length >= limit) { omitted++; return; }
      events.push(item);
      console.info(marker + JSON.stringify(item));
    };
    const activation = () => ({
      isActive: navigator.userActivation?.isActive ?? null,
      hasBeenActive: navigator.userActivation?.hasBeenActive ?? null,
    });
    const audioId = context => {
      let id = contexts.get(context);
      if (id === undefined) {
        id = ++nextContext;
        contexts.set(context, id);
        context.addEventListener("statechange", () => record({ type: "audio-statechange", contextId: id,
          state: context.state, activation: activation() }));
        record({ type: "audio-context-observed", contextId: id, state: context.state });
      }
      return id;
    };
    for (const AudioContextType of new Set([window.AudioContext, window.webkitAudioContext])) {
      if (typeof AudioContextType !== "function") continue;
      const originalResume = AudioContextType.prototype.resume;
      if (typeof originalResume !== "function") continue;
      AudioContextType.prototype.resume = function (...args) {
        const contextId = audioId(this);
        const event = { type: "audio-resume", contextId, stateBefore: this.state,
          activation: activation() };
        record(event);
        try {
          const promise = originalResume.apply(this, args);
          if (promise && typeof promise.then === "function") {
            promise.then(
              () => record({ type: "audio-resume-fulfilled", contextId, stateAfter: this.state }),
              error => record({ type: "audio-resume-rejected", contextId, stateAfter: this.state,
                error: scalar(error?.stack ?? error) }),
            );
          } else record({ type: "audio-resume-returned-non-promise", contextId, value: scalar(promise) });
          return promise;
        } catch (error) {
          record({ type: "audio-resume-threw", contextId, error: scalar(error?.stack ?? error) });
          throw error;
        }
      };
    }
    document.addEventListener("click", event => {
      const button = event.target instanceof Element ? event.target.closest(".playback-toggle") : null;
      if (button) record({ type: "play-click", trusted: event.isTrusted, activation: activation(),
        label: button.getAttribute("aria-label") });
    }, true);
    const NativeWorker = window.Worker;
    if (typeof NativeWorker === "function") {
      window.Worker = new Proxy(NativeWorker, { construct(target, args, newTarget) {
        const worker = Reflect.construct(target, args, newTarget);
        const id = ++nextWorker;
        const url = new URL(args[0], location.href).href;
        record({ type: "worker-created", workerId: id, url });
        const message = (direction, data) => record({ type: "worker-message", workerId: id, url,
          direction, channel: scalar(data?.channel), messageType: scalar(data?.type),
          requestId: scalar(data?.requestId), runRequestId: scalar(data?.runRequestId),
          protocolVersion: scalar(data?.protocolVersion) });
        worker.addEventListener("message", event => {
          if (messageTypes.has(event.data?.type)) message("from-worker", event.data);
        });
        worker.addEventListener("messageerror", () => record({ type: "worker-messageerror", workerId: id, url }));
        worker.addEventListener("error", event => record({ type: "worker-error", workerId: id, url,
          message: scalar(event.message), filename: scalar(event.filename), line: event.lineno ?? null,
          column: event.colno ?? null }));
        const originalPostMessage = worker.postMessage;
        try {
          worker.postMessage = function (data, ...rest) {
            if (messageTypes.has(data?.type)) message("to-worker", data);
            return originalPostMessage.call(this, data, ...rest);
          };
          if (worker.postMessage === originalPostMessage) throw new Error("Worker.postMessage is not patchable");
        } catch (error) {
          record({ type: "worker-postmessage-hook-failed", workerId: id, url, error: scalar(error) });
        }
        const originalTerminate = worker.terminate;
        worker.terminate = function (...args) {
          record({ type: "worker-terminate-called", workerId: id, url });
          return originalTerminate.apply(this, args);
        };
        return worker;
      } });
    }
    window.__noonPlaybackDiagnostics = {
      snapshot: () => ({ events: events.map(event => ({ ...event })), omitted }),
      activation: activation,
    };
    record({ type: "probe-installed", version: 1, activation: activation() });
    addEventListener("unhandledrejection", event => record({ type: "page-unhandledrejection",
      reason: scalar(event.reason?.stack ?? event.reason), activation: activation() }));
  });
  return { version: 1, sourceSha256: probeSha256 };
}
