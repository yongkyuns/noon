export const AUTHORING_CHANNEL = "noon.authoring";
export const AUTHORING_PROTOCOL_VERSION = 2;
export const NOON_IR_VERSION = 1;

export class PythonAuthoringClient {
  #worker;
  #nextRequestId = 0;
  #pending = new Map();
  #readyPromise;
  #resolveReady;
  #rejectReady;
  #ready = false;
  #terminated = false;

  constructor(worker = createAuthoringWorker()) {
    this.#worker = worker;
    this.#readyPromise = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    worker.addEventListener("message", (event) => this.#handleMessage(event.data));
    worker.addEventListener("error", (event) => {
      this.#fail(new Error(event.message || "Python authoring worker crashed"));
    });
  }

  ready() {
    return this.#readyPromise;
  }

  async run(source, context = {}) {
    if (typeof source !== "string" || source.trim() === "") {
      throw new TypeError("Python authoring source must be a non-empty string");
    }
    if (!isRecord(context)) {
      throw new TypeError("Python authoring context must be an object");
    }
    if (this.#terminated) {
      throw new Error("Python authoring client has been terminated");
    }

    await this.ready();
    if (this.#terminated) {
      throw new Error("Python authoring client has been terminated");
    }
    const requestId = this.#nextRequestId;
    this.#nextRequestId += 1;

    const result = new Promise((resolve, reject) => {
      this.#pending.set(requestId, { resolve, reject });
    });
    this.#worker.postMessage(
      envelope("run", {
        requestId,
        source,
        context,
      }),
    );
    return result;
  }

  terminate() {
    if (this.#terminated) {
      return;
    }
    this.#terminated = true;
    this.#worker.terminate();
    this.#fail(new Error("Python authoring client was terminated"));
  }

  #handleMessage(message) {
    try {
      validateEnvelope(message);
      if (message.type === "ready") {
        if (!this.#ready) {
          this.#ready = true;
          this.#resolveReady();
        }
        return;
      }

      if (message.type === "error") {
        const error = new Error(String(message.message || "Python authoring failed"));
        if (message.requestId === null) {
          this.#fail(error);
          return;
        }
        this.#settle(message.requestId, ({ reject }) => reject(error));
        return;
      }

      if (message.type === "patch_batch") {
        const document = validatePatchBatch(message.document);
        this.#settle(message.requestId, ({ resolve }) =>
          resolve({ kind: "patch_batch", document }),
        );
        return;
      }

      if (message.type === "scene_document") {
        const document = validateSceneDocument(message.document);
        this.#settle(message.requestId, ({ resolve }) =>
          resolve({ kind: "scene_document", document }),
        );
        return;
      }

      throw new Error(`Unknown Python authoring message type: ${message.type}`);
    } catch (error) {
      this.#fail(error);
    }
  }

  #settle(requestId, settle) {
    if (!Number.isSafeInteger(requestId) || requestId < 0) {
      throw new Error("Python authoring response has an invalid request ID");
    }
    const pending = this.#pending.get(requestId);
    if (!pending) {
      throw new Error(`Python authoring response has unknown request ID ${requestId}`);
    }
    this.#pending.delete(requestId);
    settle(pending);
  }

  #fail(error) {
    if (!this.#terminated) {
      this.#terminated = true;
      this.#worker.terminate();
    }
    if (!this.#ready) {
      this.#rejectReady(error);
    }
    for (const { reject } of this.#pending.values()) {
      reject(error);
    }
    this.#pending.clear();
  }
}

export function validatePatchBatch(batch) {
  if (!isRecord(batch)) {
    throw new Error("Python authoring result is not a PatchBatch object");
  }
  if (batch.version !== NOON_IR_VERSION) {
    throw new Error(`Unsupported Noon IR version ${batch.version}`);
  }
  if (!Number.isSafeInteger(batch.sequence) || batch.sequence < 0) {
    throw new Error("Python PatchBatch sequence must be a non-negative safe integer");
  }
  if (!Array.isArray(batch.patches)) {
    throw new Error("Python PatchBatch patches must be an array");
  }
  return batch;
}

export function validateSceneDocument(scene) {
  if (!isRecord(scene)) {
    throw new Error("Python authoring result is not a Scene object");
  }
  if (scene.version !== NOON_IR_VERSION) {
    throw new Error(`Unsupported Noon IR version ${scene.version}`);
  }
  if (!Array.isArray(scene.objects)) {
    throw new Error("Python Scene objects must be an array");
  }
  if (!Array.isArray(scene.tracks)) {
    throw new Error("Python Scene tracks must be an array");
  }
  return scene;
}

function createAuthoringWorker() {
  return new Worker(new URL("./python-worker.js", import.meta.url), {
    name: "noon-python-authoring",
    type: "module",
  });
}

function envelope(type, payload = {}) {
  return {
    channel: AUTHORING_CHANNEL,
    protocolVersion: AUTHORING_PROTOCOL_VERSION,
    type,
    ...payload,
  };
}

function validateEnvelope(message) {
  if (!isRecord(message) || message.channel !== AUTHORING_CHANNEL) {
    throw new Error("Received a message from an unknown authoring channel");
  }
  if (message.protocolVersion !== AUTHORING_PROTOCOL_VERSION) {
    throw new Error(
      `Unsupported authoring protocol version ${message.protocolVersion}`,
    );
  }
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
