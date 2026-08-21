export const AUTHORING_CHANNEL = "noon.authoring";
export const AUTHORING_PROTOCOL_VERSION = 4;
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

      if (message.type === "result") {
        const result = parseAuthoringResult(message.resultJson);
        this.#settle(message.requestId, ({ resolve }) => resolve(result));
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

export function parseAuthoringResult(resultJson) {
  if (typeof resultJson !== "string") {
    throw new Error("Python authoring result must be encoded JSON");
  }
  let result;
  try {
    result = JSON.parse(resultJson);
  } catch (error) {
    throw new Error(`Python authoring returned invalid JSON: ${error.message}`);
  }
  if (!isRecord(result)) {
    throw new Error("Python authoring result must be an object");
  }
  if (result.kind === "patch_batch") {
    return {
      kind: result.kind,
      document: validatePatchBatch(result.document),
    };
  }
  if (result.kind === "scene_document") {
    const document = validateSceneDocument(result.document);
    return {
      kind: result.kind,
      document,
      identities: validateSceneIdentities(result.identities, document),
    };
  }
  throw new Error(`Unknown Python authoring result kind: ${result.kind}`);
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

export function validateSceneIdentities(identities, scene) {
  if (!isRecord(identities)) {
    throw new Error("Python Scene identities must be an object");
  }
  validateIdentityEntries("object", identities.objects, scene.objects);
  validateIdentityEntries("track", identities.tracks, scene.tracks);
  return identities;
}

function validateIdentityEntries(kind, entries, definitions) {
  if (!Array.isArray(entries) || entries.length !== definitions.length) {
    throw new Error(`Python Scene ${kind} identities must match its definitions`);
  }
  const definitionIds = new Set(definitions.map(({ id }) => id));
  const ids = new Set();
  const keys = new Set();
  for (const entry of entries) {
    if (
      !isRecord(entry) ||
      !Number.isSafeInteger(entry.id) ||
      entry.id < 0 ||
      !definitionIds.has(entry.id)
    ) {
      throw new Error(`Python Scene has an invalid ${kind} identity ID`);
    }
    if (typeof entry.key !== "string" || entry.key.trim() === "") {
      throw new Error(`Python Scene has an invalid ${kind} identity key`);
    }
    if (ids.has(entry.id) || keys.has(entry.key)) {
      throw new Error(`Python Scene has duplicate ${kind} identities`);
    }
    ids.add(entry.id);
    keys.add(entry.key);
  }
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
