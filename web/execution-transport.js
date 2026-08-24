export const EXECUTION_TRANSPORT_CHANNEL = "noon.execution";
export const EXECUTION_TRANSPORT_VERSION = 1;
export const EXECUTION_TRANSPORT_SHARED = "shared";
export const EXECUTION_TRANSPORT_TRANSFERABLE = "transferable";

const SLOT_FREE = 0;
const SLOT_WRITING = 1;
const SLOT_READY = 2;
const HEADER_WORDS = 8;
const SLOT_STATE_0 = 0;
const SLOT_STATE_1 = 1;
const SLOT_LENGTH_0 = 2;
const SLOT_LENGTH_1 = 3;
const WAKE_COUNTER = 4;
const BACKPRESSURE_COUNTER = 5;
const DEFAULT_SLOT_CAPACITY = 1024 * 1024;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function selectExecutionTransportMode(scope = globalThis) {
  return scope.crossOriginIsolated === true && typeof scope.SharedArrayBuffer === "function"
    ? EXECUTION_TRANSPORT_SHARED
    : EXECUTION_TRANSPORT_TRANSFERABLE;
}

export function executionDeltaMetadata(json) {
  if (typeof json !== "string") {
    throw new TypeError("execution delta must be a JSON string");
  }
  let delta;
  try {
    delta = JSON.parse(json);
  } catch (error) {
    throw new Error(`execution delta is invalid JSON: ${error.message}`);
  }
  if (!isRecord(delta) || delta.channel !== EXECUTION_TRANSPORT_CHANNEL) {
    throw new Error("execution delta has an invalid channel");
  }
  if (delta.protocol_version !== EXECUTION_TRANSPORT_VERSION) {
    throw new Error(`unsupported execution transport version ${delta.protocol_version}`);
  }
  if (!Number.isSafeInteger(delta.session) || delta.session < 0) {
    throw new Error("execution delta has an invalid session");
  }
  if (!Number.isSafeInteger(delta.sequence) || delta.sequence < 0) {
    throw new Error("execution delta has an invalid sequence");
  }
  if (typeof delta.snapshot !== "boolean") {
    throw new Error("execution delta snapshot flag must be boolean");
  }
  return {
    session: delta.session,
    sequence: delta.sequence,
    snapshot: delta.snapshot,
  };
}

export function createSharedExecutionMailbox(slotCapacity = DEFAULT_SLOT_CAPACITY) {
  if (!Number.isSafeInteger(slotCapacity) || slotCapacity <= 0) {
    throw new TypeError("shared execution mailbox capacity must be a positive integer");
  }
  if (typeof SharedArrayBuffer !== "function") {
    throw new Error("SharedArrayBuffer is unavailable");
  }
  const headerBytes = HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT;
  const buffer = new SharedArrayBuffer(headerBytes + slotCapacity * 2);
  return { buffer, slotCapacity };
}

export class SharedExecutionDeltaWriter {
  #header;
  #bytes;
  #slotCapacity;

  constructor(mailbox) {
    validateSharedMailbox(mailbox);
    this.#slotCapacity = mailbox.slotCapacity;
    this.#header = new Int32Array(mailbox.buffer, 0, HEADER_WORDS);
    this.#bytes = new Uint8Array(
      mailbox.buffer,
      HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT,
    );
  }

  canSend() {
    return (
      Atomics.load(this.#header, SLOT_STATE_0) === SLOT_FREE ||
      Atomics.load(this.#header, SLOT_STATE_1) === SLOT_FREE
    );
  }

  send(json) {
    executionDeltaMetadata(json);
    const payload = encoder.encode(json);
    if (payload.byteLength > this.#slotCapacity) {
      throw new Error(
        `execution delta ${payload.byteLength} bytes exceeds shared slot capacity ${this.#slotCapacity}`,
      );
    }
    for (let slot = 0; slot < 2; slot += 1) {
      const stateIndex = slot === 0 ? SLOT_STATE_0 : SLOT_STATE_1;
      if (Atomics.compareExchange(this.#header, stateIndex, SLOT_FREE, SLOT_WRITING) !== SLOT_FREE) {
        continue;
      }
      const offset = slot * this.#slotCapacity;
      this.#bytes.set(payload, offset);
      Atomics.store(
        this.#header,
        slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1,
        payload.byteLength,
      );
      Atomics.store(this.#header, stateIndex, SLOT_READY);
      Atomics.add(this.#header, WAKE_COUNTER, 1);
      Atomics.notify(this.#header, WAKE_COUNTER);
      return true;
    }
    Atomics.add(this.#header, BACKPRESSURE_COUNTER, 1);
    return false;
  }

  backpressureCount() {
    return Atomics.load(this.#header, BACKPRESSURE_COUNTER);
  }
}

export class SharedExecutionDeltaReader {
  #header;
  #bytes;
  #slotCapacity;

  constructor(mailbox) {
    validateSharedMailbox(mailbox);
    this.#slotCapacity = mailbox.slotCapacity;
    this.#header = new Int32Array(mailbox.buffer, 0, HEADER_WORDS);
    this.#bytes = new Uint8Array(
      mailbox.buffer,
      HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT,
    );
  }

  drain(apply) {
    if (typeof apply !== "function") {
      throw new TypeError("shared execution mailbox drain requires an apply callback");
    }
    const ready = [];
    for (let slot = 0; slot < 2; slot += 1) {
      const stateIndex = slot === 0 ? SLOT_STATE_0 : SLOT_STATE_1;
      if (Atomics.load(this.#header, stateIndex) !== SLOT_READY) {
        continue;
      }
      const length = Atomics.load(
        this.#header,
        slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1,
      );
      if (length < 0 || length > this.#slotCapacity) {
        Atomics.store(this.#header, stateIndex, SLOT_FREE);
        throw new Error(`shared execution mailbox slot ${slot} has invalid length ${length}`);
      }
      const offset = slot * this.#slotCapacity;
      const json = decoder.decode(this.#bytes.slice(offset, offset + length));
      ready.push({ slot, stateIndex, json, metadata: executionDeltaMetadata(json) });
    }
    ready.sort((left, right) => left.metadata.sequence - right.metadata.sequence);
    for (const item of ready) {
      try {
        apply(item.json, item.metadata);
      } finally {
        Atomics.store(
          this.#header,
          item.slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1,
          0,
        );
        Atomics.store(this.#header, item.stateIndex, SLOT_FREE);
      }
    }
    return ready.length;
  }
}

export class TransferableExecutionDeltaSender {
  #port;
  #inFlight = 0;
  #maxInFlight;
  #onWritable;
  #backpressure = 0;

  constructor(port, { maxInFlight = 2, onWritable = null } = {}) {
    if (!port || typeof port.postMessage !== "function") {
      throw new TypeError("transferable execution sender requires a MessagePort-like object");
    }
    if (!Number.isSafeInteger(maxInFlight) || maxInFlight <= 0) {
      throw new TypeError("maxInFlight must be a positive integer");
    }
    this.#port = port;
    this.#maxInFlight = maxInFlight;
    this.#onWritable = onWritable;
    port.addEventListener?.("message", (event) => this.#handleMessage(event.data));
    port.on?.("message", (data) => this.#handleMessage(data));
    port.start?.();
  }

  canSend() {
    return this.#inFlight < this.#maxInFlight;
  }

  send(json) {
    const metadata = executionDeltaMetadata(json);
    if (this.#inFlight >= this.#maxInFlight) {
      this.#backpressure += 1;
      return false;
    }
    const payload = encoder.encode(json);
    const buffer = payload.buffer.slice(
      payload.byteOffset,
      payload.byteOffset + payload.byteLength,
    );
    this.#inFlight += 1;
    this.#port.postMessage(
      {
        type: "execution_delta",
        session: metadata.session,
        sequence: metadata.sequence,
        buffer,
      },
      [buffer],
    );
    return true;
  }

  inFlight() {
    return this.#inFlight;
  }

  backpressureCount() {
    return this.#backpressure;
  }

  #handleMessage(message) {
    if (!isRecord(message) || message.type !== "execution_ack") {
      return;
    }
    if (this.#inFlight === 0) {
      return;
    }
    const wasBlocked = this.#inFlight >= this.#maxInFlight;
    this.#inFlight -= 1;
    if (wasBlocked && this.#inFlight < this.#maxInFlight) {
      this.#onWritable?.();
    }
  }
}

export class TransferableExecutionDeltaReceiver {
  #port;
  #apply;
  #onWritable;

  constructor(port, apply, { onWritable = null } = {}) {
    if (!port || typeof port.postMessage !== "function") {
      throw new TypeError("transferable execution receiver requires a MessagePort-like object");
    }
    if (typeof apply !== "function") {
      throw new TypeError("transferable execution receiver requires an apply callback");
    }
    this.#port = port;
    this.#apply = apply;
    this.#onWritable = onWritable;
    port.addEventListener?.("message", (event) => this.#handleMessage(event.data));
    port.on?.("message", (data) => this.#handleMessage(data));
    port.start?.();
  }

  #handleMessage(message) {
    if (!isRecord(message) || message.type !== "execution_delta") {
      return;
    }
    if (!(message.buffer instanceof ArrayBuffer)) {
      throw new Error("transferable execution delta payload must be an ArrayBuffer");
    }
    const json = decoder.decode(new Uint8Array(message.buffer));
    const metadata = executionDeltaMetadata(json);
    try {
      this.#apply(json, metadata);
    } finally {
      this.#port.postMessage({
        type: "execution_ack",
        session: metadata.session,
        sequence: metadata.sequence,
      });
      this.#onWritable?.();
    }
  }
}

function validateSharedMailbox(mailbox) {
  if (!isRecord(mailbox)) {
    throw new TypeError("shared execution mailbox must be an object");
  }
  if (!(mailbox.buffer instanceof SharedArrayBuffer)) {
    throw new TypeError("shared execution mailbox buffer must be SharedArrayBuffer");
  }
  if (!Number.isSafeInteger(mailbox.slotCapacity) || mailbox.slotCapacity <= 0) {
    throw new TypeError("shared execution mailbox capacity must be a positive integer");
  }
  const expected =
    HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT + mailbox.slotCapacity * 2;
  if (mailbox.buffer.byteLength !== expected) {
    throw new Error(
      `shared execution mailbox has ${mailbox.buffer.byteLength} bytes; expected ${expected}`,
    );
  }
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
