// Shared browser endpoint doubles; semantic behavior remains in Rust qualification.
export class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;
  transfers = 0;
  transferred = false;
  replacement = null;
  className = "scene-canvas";
  id = "scene";
  remove() {}
  transferControlToOffscreen() {
    if (this.transferred) throw new Error("canvas transferred twice");
    this.transferred = true;
    this.transfers += 1;
    return { width: this.width, height: this.height };
  }
  cloneNode() {
    return new FakeCanvas();
  }
  replaceWith(canvas) { this.replacement = canvas; }
}

export class FakePort {
  listeners = new Map();
  messages = [];
  peer = null;
  closed = false;
  started = false;
  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }
  postMessage(message) {
    this.messages.push(message);
    queueMicrotask(() => this.peer?.emitMessage(message));
  }
  start() {
    this.started = true;
  }
  close() {
    this.closed = true;
  }
  emitError(message = "worker crashed", details = {}) {
    for (const listener of this.listeners.get("error") ?? []) listener({ message, ...details });
  }
  emitMessageError() {
    for (const listener of this.listeners.get("messageerror") ?? []) listener({});
  }
  emitMessage(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }
}

export class FakeMessageChannel {
  constructor() {
    this.port1 = new FakePort();
    this.port2 = new FakePort();
    this.port1.peer = this.port2;
    this.port2.peer = this.port1;
  }
}

export class FakeWorker {
  static instances = [];
  static failNextName = null;
  listeners = new Map();
  messages = [];
  terminated = false;
  constructor(_url, options = {}) {
    this.name = options.name ?? "";
    if (FakeWorker.failNextName === this.name) {
      FakeWorker.failNextName = null;
      throw new Error(`${this.name} constructor failed`);
    }
    FakeWorker.instances.push(this);
  }
  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) ?? [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }
  postMessage(message, transfer = []) {
    this.messages.push({ message, transfer });
  }
  terminate() {
    this.terminated = true;
  }
  emitError(message = "worker crashed", details = {}) {
    for (const listener of this.listeners.get("error") ?? []) listener({ message, ...details });
  }
  emitMessageError() {
    for (const listener of this.listeners.get("messageerror") ?? []) listener({});
  }
  emitMessage(message) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }
}

export class FakeResizeObserver {
  static instances = [];
  active = false;
  constructor(callback) { this.callback = callback; FakeResizeObserver.instances.push(this); }
  observe(canvas) { this.canvas = canvas; this.active = true; }
  disconnect() { this.active = false; }
  deliver() { this.callback(); }
}
export class FakeSemanticAuthoringClient {
  attachments = [];
  stoppedContexts = [];
  releasedContexts = [];
  failContext = null;
  autoRespond = true;

  async attachSemanticExecution(contextId, controlPort, renderPort, options) {
    this.attachments.push({ contextId, controlPort, renderPort, options });
    if (contextId === this.failContext) {
      throw new Error("semantic context rejected");
    }
    controlPort.addEventListener("message", ({ data: message }) => {
      if (message.type === "stop") {
        this.stoppedContexts.push(contextId);
        return;
      }
      if (!this.autoRespond) return;
      const state = {
        requestId: message.requestId,
        time: message.type === "seek" ? message.time : 0,
        playing: message.type === "pause" ? false : true,
      };
      controlPort.postMessage(envelope("noon.engine", message.type, state));
    });
    controlPort.start();
    // The real Python endpoint queues transport setup and sequence-zero snapshot
    // on renderPort before it publishes readiness on this control endpoint.
    renderPort.postMessage({ type: "test-sequence-zero-snapshot", session: options.session });
    controlPort.postMessage(
      envelope("noon.engine", "ready", {
        transportMode: options.transportMode,
        semantic: true,
      }),
    );
    return { type: "semantic_execution_attached", contextId };
  }

  async releaseSemanticExecution(contextId) {
    this.releasedContexts.push(contextId);
    return { type: "semantic_execution_released", contextId };
  }
}

function envelope(channel, type, payload = {}) {
  return { channel, protocolVersion: 1, type, ...payload };
}
