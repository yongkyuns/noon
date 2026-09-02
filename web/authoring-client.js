export const AUTHORING_CHANNEL = "noon.authoring";
export const AUTHORING_PROTOCOL_VERSION = 5;
export const NOON_IR_VERSION = 1;
export const SCENE_SPEC_VERSION = 1;
export const RETAINED_AUTHORING_CHANNEL = "noon.authoring.retained";
export const RETAINED_AUTHORING_VERSION = 2;

export class PythonAuthoringClient {
  #worker;
  #nextRequestId = 0;
  #pending = new Map();
  #readyPromise;
  #resolveReady;
  #rejectReady;
  #ready = false;
  #terminated = false;
  #staleResponses = 0;

  constructor(worker = createAuthoringWorker()) {
    this.#worker = worker;
    this.#readyPromise = new Promise((resolve, reject) => {
      this.#resolveReady = resolve;
      this.#rejectReady = reject;
    });
    worker.addEventListener("message", (event) => this.#handleMessage(event.data));
    worker.addEventListener("error", (event) => {
      const details = [
        event.message,
        event.error?.stack,
        event.filename && `${event.filename}:${event.lineno ?? 0}:${event.colno ?? 0}`,
      ].filter(Boolean);
      this.#fail(new Error(details.join("\n") || "Python authoring worker crashed"));
    });
  }

  get terminated() {
    return this.#terminated;
  }

  get diagnostics() {
    return Object.freeze({
      nextRequestId: this.#nextRequestId,
      pendingRequests: this.#pending.size,
      staleResponses: this.#staleResponses,
      terminated: this.#terminated,
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
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    this.#worker.postMessage(
      envelope("run", {
        requestId,
        source,
        context,
      }),
    );
    return result;
  }

  async runCallbackPhase(sessionId, frame, sequence) {
    if (!Number.isSafeInteger(sessionId) || sessionId < 0) {
      throw new TypeError("callback session ID must be a non-negative safe integer");
    }
    if (!isRecord(frame)) {
      throw new TypeError("callback frame must be an object");
    }
    if (!Number.isSafeInteger(sequence) || sequence < 0) {
      throw new TypeError("callback patch sequence must be a non-negative safe integer");
    }
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    this.#worker.postMessage(
      envelope("callback_phase", {
        requestId,
        sessionId,
        frame,
        sequence,
      }),
    );
    return result;
  }

  async attachEnginePort(port) {
    if (!(port instanceof MessagePort)) {
      throw new TypeError("engine host attachment requires a MessagePort");
    }
    await this.ready();
    const requestId = this.#beginRequest();
    const result = this.#resultFor(requestId);
    this.#worker.postMessage(envelope("attach_engine_port", { requestId, port }), [port]);
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

  #beginRequest() {
    if (this.#terminated) {
      throw new Error("Python authoring client has been terminated");
    }
    const requestId = this.#nextRequestId;
    this.#nextRequestId += 1;
    return requestId;
  }

  #resultFor(requestId) {
    return new Promise((resolve, reject) => {
      this.#pending.set(requestId, { resolve, reject });
    });
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
        const details = [
          message.message,
          message.diagnostic?.stack,
        ].filter(Boolean);
        const error = new Error(String(details.join("\n") || "Python authoring failed"));
        if (message.requestId === null) {
          this.#fail(error);
          return;
        }
        this.#settle(message.requestId, ({ reject }) => reject(error));
        return;
      }

      if (message.type === "result") {
        this.#settle(message.requestId, ({ resolve }) => {
          resolve(parseAuthoringResult(message.resultJson));
        });
        return;
      }

      if (message.type === "callback_result") {
        this.#settle(message.requestId, ({ resolve }) => {
          resolve(parsePatchBatchJson(message.patchBatchJson));
        });
        return;
      }

      if (message.type === "host_port_attached") {
        this.#settle(message.requestId, ({ resolve }) => resolve(message));
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
      if (requestId < this.#nextRequestId) {
        this.#staleResponses += 1;
        return false;
      }
      throw new Error(`Python authoring response has unissued request ID ${requestId}`);
    }
    settle(pending);
    this.#pending.delete(requestId);
    return true;
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
    const retainedDocument = validateRetainedAuthoringDocument(result.retained_document);
    const sceneSpec = validateSceneSpec(result.scene_spec);
    if (sceneSpec === null) {
      throw new Error("Python Scene result must include canonical SceneSpec");
    }
    const parsed = {
      kind: result.kind,
      document,
      sceneSpec,
      duration: validateSceneDuration(result.duration),
      identities: validateSceneIdentities(result.identities, document),
      callbacks: validateCallbackSession(result.callbacks, document),
    };
    if (retainedDocument !== null) {
      parsed.retainedDocument = retainedDocument;
    }
    return parsed;
  }
  throw new Error(`Unknown Python authoring result kind: ${result.kind}`);
}

export function parsePatchBatchJson(json) {
  if (typeof json !== "string") {
    throw new Error("Python callback result must be encoded JSON");
  }
  let batch;
  try {
    batch = JSON.parse(json);
  } catch (error) {
    throw new Error(`Python callback returned invalid JSON: ${error.message}`);
  }
  return validatePatchBatch(batch);
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

  validateDefinitionIds("object", scene.objects);
  validateDefinitionIds("track", scene.tracks);
  return scene;
}

export function validateSceneSpec(sceneSpec) {
  if (sceneSpec === null || sceneSpec === undefined) {
    return null;
  }
  if (!isRecord(sceneSpec)) {
    throw new Error("Python canonical SceneSpec result must be an object");
  }
  if (sceneSpec.version !== SCENE_SPEC_VERSION) {
    throw new Error(`Unsupported canonical SceneSpec version ${sceneSpec.version}`);
  }
  if (!Array.isArray(sceneSpec.objects)) {
    throw new Error("Python canonical SceneSpec objects must be an array");
  }
  if (!Array.isArray(sceneSpec.tracks)) {
    throw new Error("Python canonical SceneSpec tracks must be an array");
  }

  const objectIds = new Set();
  for (const object of sceneSpec.objects) {
    if (!isRecord(object) || !Number.isSafeInteger(object.id) || object.id < 0) {
      throw new Error("Python canonical SceneSpec object has an invalid object ID");
    }
    if (objectIds.has(object.id)) {
      throw new Error("Python canonical SceneSpec has duplicate object IDs");
    }
    objectIds.add(object.id);
  }
  if (
    sceneSpec.camera_object !== null &&
    sceneSpec.camera_object !== undefined &&
    (!Number.isSafeInteger(sceneSpec.camera_object) ||
      sceneSpec.camera_object < 0 ||
      !objectIds.has(sceneSpec.camera_object))
  ) {
    throw new Error("Python canonical SceneSpec has an invalid camera object");
  }
  return sceneSpec;
}

export function validateRetainedAuthoringDocument(document) {
  if (document === null || document === undefined) {
    return null;
  }
  if (!isRecord(document)) {
    throw new Error("Python retained authoring result must be an object");
  }
  if (document.channel !== RETAINED_AUTHORING_CHANNEL) {
    throw new Error(`Invalid retained authoring channel ${document.channel}`);
  }
  if (document.protocol_version !== RETAINED_AUTHORING_VERSION) {
    throw new Error(
      `Unsupported retained authoring protocol version ${document.protocol_version}`,
    );
  }
  if (!Array.isArray(document.objects)) {
    throw new Error("Python retained authoring objects must be an array");
  }

  const objectIds = new Set();
  const orders = new Set();
  for (const object of document.objects) {
    if (!isRecord(object) || !Number.isSafeInteger(object.object) || object.object < 0) {
      throw new Error("Python retained authoring object has an invalid object ID");
    }
    if (!Number.isSafeInteger(object.order) || object.order < 0) {
      throw new Error("Python retained authoring object has an invalid painter order");
    }
    if (objectIds.has(object.object)) {
      throw new Error("Python retained authoring document has duplicate object IDs");
    }
    if (orders.has(object.order)) {
      throw new Error("Python retained authoring document has duplicate painter orders");
    }
    objectIds.add(object.object);
    orders.add(object.order);
    validateRetainedTextSpec(object.text);
  }
  return document;
}

function validateRetainedTextSpec(text) {
  if (!isRecord(text) || typeof text.source !== "string") {
    throw new Error("Python retained text source must be a string");
  }
  if (!isRecord(text.backend) || typeof text.backend.kind !== "string") {
    throw new Error("Python retained text backend is malformed");
  }
  if (text.backend.kind === "native") {
    if (typeof text.backend.font_family !== "string" || text.backend.font_family.trim() === "") {
      throw new Error("Python retained native text font family must be non-empty");
    }
    if (
      !Number.isFinite(text.backend.line_spacing) ||
      (text.backend.line_spacing !== -1 && text.backend.line_spacing <= -1)
    ) {
      throw new Error("Python retained native text line spacing must be -1 or greater than -1");
    }
  } else if (text.backend.kind === "typst") {
    if (text.source.length === 0) {
      throw new Error("Python retained Typst source must be a non-empty string");
    }
    if (typeof text.backend.math !== "boolean") {
      throw new Error("Python retained Typst math flag must be boolean");
    }
  } else {
    throw new Error(`Unsupported Python retained text backend ${text.backend.kind}`);
  }
  if (!Number.isFinite(text.font_size) || text.font_size <= 0) {
    throw new Error("Python retained text font size must be finite and positive");
  }
  if (!Number.isFinite(text.opacity) || text.opacity < 0 || text.opacity > 1) {
    throw new Error("Python retained text opacity must be between zero and one");
  }
  if (!isRecord(text.transform) || !isRecord(text.transform.translation) || !isRecord(text.transform.scale)) {
    throw new Error("Python retained text transform is malformed");
  }
  for (const value of [
    text.transform.translation.x,
    text.transform.translation.y,
    text.transform.scale.x,
    text.transform.scale.y,
    text.transform.rotation,
  ]) {
    if (!Number.isFinite(value)) {
      throw new Error("Python retained text transform must be finite");
    }
  }
  if (!isRecord(text.color)) {
    throw new Error("Python retained text color is malformed");
  }
  for (const value of [text.color.red, text.color.green, text.color.blue, text.color.alpha]) {
    if (!Number.isFinite(value)) {
      throw new Error("Python retained text color must be finite");
    }
  }
}

export function validateSceneDuration(duration) {
  if (!Number.isFinite(duration) || duration < 0) {
    throw new Error("Python Scene duration must be finite and non-negative");
  }
  return duration;
}

export function validateSceneIdentities(identities, scene) {
  if (!isRecord(identities)) {
    throw new Error("Python Scene identities must be an object");
  }
  validateIdentityEntries("object", identities.objects, scene.objects);
  validateIdentityEntries("track", identities.tracks, scene.tracks);
  return identities;
}

export function validateCallbackSession(callbacks, scene) {
  if (callbacks === null || callbacks === undefined) {
    return null;
  }
  if (!isRecord(callbacks)) {
    throw new Error("Python Scene callback session must be an object");
  }
  if (!Number.isSafeInteger(callbacks.session_id) || callbacks.session_id < 0) {
    throw new Error("Python Scene callback session has an invalid session ID");
  }
  if (!Array.isArray(callbacks.slots) || callbacks.slots.length === 0) {
    throw new Error("Python Scene callback session must contain callback slots");
  }
  const objectIds = new Set(scene.objects.map(({ id }) => id));
  const callbackIds = new Set();
  for (const slot of callbacks.slots) {
    if (!isRecord(slot) || !Number.isSafeInteger(slot.id) || slot.id < 0) {
      throw new Error("Python Scene has an invalid callback slot ID");
    }
    if (callbackIds.has(slot.id)) {
      throw new Error("Python Scene has duplicate callback slot IDs");
    }
    callbackIds.add(slot.id);
    if (!Array.isArray(slot.objects)) {
      throw new Error("Python Scene callback slot objects must be an array");
    }
    const seen = new Set();
    for (const object of slot.objects) {
      if (!Number.isSafeInteger(object) || object < 0 || !objectIds.has(object)) {
        throw new Error("Python Scene callback slot references an invalid object");
      }
      if (seen.has(object)) {
        throw new Error("Python Scene callback slot contains duplicate objects");
      }
      seen.add(object);
    }
  }
  return callbacks;
}

function validateDefinitionIds(kind, definitions) {
  const ids = new Set();
  for (const definition of definitions) {
    if (
      !isRecord(definition) ||
      !Number.isSafeInteger(definition.id) ||
      definition.id < 0
    ) {
      throw new Error(`Python Scene has an invalid ${kind} ID`);
    }
    if (ids.has(definition.id)) {
      throw new Error(`Python Scene has duplicate ${kind} IDs`);
    }
    ids.add(definition.id);
  }
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
