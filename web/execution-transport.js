export const RETAINED_EXECUTION_TRANSPORT_CHANNEL = "noon.execution.retained";
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
// Two slots reserve 4 MiB per isolated execution. This covers the gallery's
// 600-object morph deltas without the 32 MiB slots used by stress experiments.
export const DEFAULT_SHARED_SLOT_CAPACITY = 2 * 1024 * 1024;
// Bound the encoded size of one shared packet. This is not a bound on source
// JSON, decoder/encoder temporaries, renderer or total process memory.
export const MAX_SHARED_EXECUTION_DELTA_BYTES = 64 * 1024 * 1024;
const CHUNK_MAGIC = 0xffffffff;
const CHUNK_VERSION = 1;
const CHUNK_HEADER_BYTES = 32;
const encoder = new TextEncoder();
const decoder = new TextDecoder();
const validatedDeltaJson = new WeakMap();

export function selectExecutionTransportMode(scope = globalThis) {
  return scope.crossOriginIsolated === true && typeof scope.SharedArrayBuffer === "function"
    ? EXECUTION_TRANSPORT_SHARED
    : EXECUTION_TRANSPORT_TRANSFERABLE;
}

export function executionDeltaMetadata(json) {
  return parseExecutionDeltaMetadata(json);
}

// Prepare a one-shot producer handoff. Unlike the public metadata reader, this
// temporarily ties an opaque result to its exact JSON so transport send can
// skip a second producer parse. Send consumes the association immediately.
export function prepareExecutionDeltaMetadataForSend(json) {
  const metadata = parseExecutionDeltaMetadata(json);
  validatedDeltaJson.set(metadata, json);
  return metadata;
}

// Only accepts the atomic header/body carrier from drainDeltaTransportJson.
// The body remains the ordinary worker delta; both receiving transports still
// parse/validate it before admission. Generic/untrusted JSON uses the reader
// above, which never treats a supplied header as proof of its body.
export function prepareExecutionDeltaTransportForSend(packet) {
  if (typeof packet !== "string") {
    throw new TypeError("worker publication must be a header/body string");
  }
  const separator = packet.indexOf("\n");
  if (separator < 1 || separator > 512 || separator === packet.length - 1) {
    throw new Error("worker publication has an invalid bounded metadata header");
  }
  const metadata = parseExecutionDeltaMetadata(packet.slice(0, separator));
  const json = packet.slice(separator + 1);
  validatedDeltaJson.set(metadata, json);
  return { json, metadata };
}

function parseExecutionDeltaMetadata(json) {
  if (typeof json !== "string") {
    throw new TypeError("execution delta must be a JSON string");
  }
  let delta;
  try {
    delta = JSON.parse(json);
  } catch (error) {
    throw new Error(`execution delta is invalid JSON: ${error.message}`);
  }
  if (!isRecord(delta) || delta.channel !== RETAINED_EXECUTION_TRANSPORT_CHANNEL) {
    throw new Error("execution delta has an invalid channel");
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
  const view = delta.pointer_view;
  if (view !== undefined && (view === null || !Number.isSafeInteger(view.revision) || view.revision < 0 ||
      !Number.isFinite(view.width) || !Number.isFinite(view.height) || view.width <= 0 || view.height <= 0)) {
    throw new Error("execution delta has an invalid pointer view");
  }
  const metadata = Object.freeze({
    session: delta.session,
    sequence: delta.sequence,
    snapshot: delta.snapshot,
    ...(view === undefined ? {} : { pointerView: Object.freeze({
      revision: view.revision, width: view.width, height: view.height,
    }) }),
  });
  return metadata;
}

function metadataForSend(json, validatedMetadata) {
  // Consume the temporary exact-string association even when it mismatches,
  // so a retained or misused handoff cannot pin a large delta indefinitely.
  if (validatedMetadata !== null && typeof validatedMetadata === "object") {
    const validatedJson = validatedDeltaJson.get(validatedMetadata);
    if (validatedJson !== undefined) {
      validatedDeltaJson.delete(validatedMetadata);
      if (typeof json === "string" && validatedJson === json) return validatedMetadata;
    }
  }
  return executionDeltaMetadata(json);
}

export function decodeTransferableExecutionDelta(message) {
  if (!isRecord(message) || message.type !== "execution_delta") {
    throw new Error("transferable execution message must be an execution_delta envelope");
  }
  if (!(message.buffer instanceof ArrayBuffer)) {
    throw new Error("transferable execution delta payload must be an ArrayBuffer");
  }
  const json = decoder.decode(new Uint8Array(message.buffer));
  const metadata = executionDeltaMetadata(json);
  if (metadata.session !== message.session || metadata.sequence !== message.sequence) {
    throw new Error("transferable execution delta metadata does not match its envelope");
  }
  return { json, metadata };
}

export function createSharedExecutionMailbox(slotCapacity = DEFAULT_SHARED_SLOT_CAPACITY) {
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
  #pending = null;
  #closed = false;

  constructor(mailbox) {
    validateSharedMailbox(mailbox);
    this.#slotCapacity = mailbox.slotCapacity;
    this.#header = new Int32Array(mailbox.buffer, 0, HEADER_WORDS);
    this.#bytes = new Uint8Array(mailbox.buffer, HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT);
  }

  canSend() {
    return !this.#closed && this.#pending === null && (
      Atomics.load(this.#header, SLOT_STATE_0) === SLOT_FREE ||
      Atomics.load(this.#header, SLOT_STATE_1) === SLOT_FREE
    );
  }

  send(json, validatedMetadata = null) {
    if (this.#closed) throw new Error("shared execution writer is closed");
    if (typeof json === "string" && json.length > MAX_SHARED_EXECUTION_DELTA_BYTES) {
      throw new Error("shared execution delta exceeds packet byte limit");
    }
    const metadata = metadataForSend(json, validatedMetadata);
    if (!this.canSend()) {
      Atomics.add(this.#header, BACKPRESSURE_COUNTER, 1);
      return false;
    }
    const payload = encoder.encode(json);
    if (payload.byteLength > MAX_SHARED_EXECUTION_DELTA_BYTES) {
      throw new Error("shared execution delta exceeds packet byte limit");
    }
    if (payload.byteLength <= this.#slotCapacity) return this.#write(payload);
    if (this.#slotCapacity <= CHUNK_HEADER_BYTES) {
      throw new Error("shared execution slot is too small for chunk framing");
    }
    // Keep exactly this encoded packet, never regenerate a drained Rust delta.
    // Partial chunks release physical slots, not publication/presentation receipts.
    this.#pending = { payload, metadata, offset: 0 };
    this.flush();
    return true;
  }

  // Called by the existing renderer transport-credit event. No polling task or
  // source/semantic work is scheduled while both physical slots remain owned.
  flush() {
    let written = 0;
    while (!this.#closed && this.#pending !== null) {
      const pending = this.#pending;
      const end = Math.min(pending.payload.byteLength, pending.offset + this.#slotCapacity - CHUNK_HEADER_BYTES);
      if (!this.#write(pending.payload.subarray(pending.offset, end), pending)) break;
      pending.offset = end;
      written++;
      if (end === pending.payload.byteLength) this.#pending = null;
    }
    return written;
  }

  #write(payload, chunk = null) {
    for (let slot = 0; slot < 2; slot++) {
      const stateIndex = slot === 0 ? SLOT_STATE_0 : SLOT_STATE_1;
      if (Atomics.compareExchange(this.#header, stateIndex, SLOT_FREE, SLOT_WRITING) !== SLOT_FREE) continue;
      const offset = slot * this.#slotCapacity;
      if (chunk !== null) {
        const view = new DataView(this.#bytes.buffer, this.#bytes.byteOffset + offset, CHUNK_HEADER_BYTES);
        view.setUint32(0, CHUNK_MAGIC, true);
        view.setUint32(4, chunk.payload.byteLength, true);
        view.setUint32(8, chunk.offset, true);
        view.setUint32(12, CHUNK_VERSION, true);
        view.setFloat64(16, chunk.metadata.session, true);
        view.setFloat64(24, chunk.metadata.sequence, true);
      }
      const headerBytes = chunk === null ? 0 : CHUNK_HEADER_BYTES;
      this.#bytes.set(payload, offset + headerBytes);
      Atomics.store(this.#header, slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1, headerBytes + payload.byteLength);
      Atomics.store(this.#header, stateIndex, SLOT_READY);
      Atomics.add(this.#header, WAKE_COUNTER, 1);
      Atomics.notify(this.#header, WAKE_COUNTER);
      return true;
    }
    return false;
  }

  close() {
    this.#closed = true;
    this.#pending = null;
  }

  backpressureCount() { return Atomics.load(this.#header, BACKPRESSURE_COUNTER); }
}

export class SharedExecutionDeltaReader {
  #header;
  #bytes;
  #slotCapacity;
  #pending = null;
  #closed = false;

  constructor(mailbox) {
    validateSharedMailbox(mailbox);
    this.#slotCapacity = mailbox.slotCapacity;
    this.#header = new Int32Array(mailbox.buffer, 0, HEADER_WORDS);
    this.#bytes = new Uint8Array(mailbox.buffer, HEADER_WORDS * Int32Array.BYTES_PER_ELEMENT);
  }

  drain(apply) {
    if (typeof apply !== "function") throw new TypeError("shared execution mailbox drain requires an apply callback");
    if (this.#closed) return 0;
    const ready = [];
    for (let slot = 0; slot < 2; slot++) {
      const stateIndex = slot === 0 ? SLOT_STATE_0 : SLOT_STATE_1;
      if (Atomics.load(this.#header, stateIndex) !== SLOT_READY) continue;
      try { ready.push(this.#read(slot, stateIndex)); }
      catch (error) { this.#pending = null; this.#release(slot, stateIndex); throw error; }
    }
    ready.sort((left, right) => left.metadata.sequence - right.metadata.sequence ||
      (left.chunk?.offset ?? 0) - (right.chunk?.offset ?? 0));
    let consumed = 0;
    for (const item of ready) {
      try {
        if (item.chunk) {
          const { total, offset } = item.chunk;
          if (this.#pending === null) {
            if (offset !== 0) throw new Error("shared execution chunk has no initial packet");
            this.#pending = { total, received: 0, ...item.metadata,
              bytes: new Uint8Array(total), json: null, metadata: null, finalOffset: null };
          }
          const pending = this.#pending;
          if (pending.total !== total || pending.session !== item.metadata.session || pending.sequence !== item.metadata.sequence) {
            throw new Error("shared execution chunk belongs to another packet");
          }
          if (pending.json === null) {
            if (offset !== pending.received) throw new Error("shared execution chunk is out of order");
            pending.bytes.set(item.payload, offset);
            pending.received += item.payload.byteLength;
            if (pending.received === total) {
              pending.json = decoder.decode(pending.bytes);
              pending.bytes = null;
              pending.metadata = executionDeltaMetadata(pending.json);
              pending.finalOffset = offset;
              if (pending.metadata.session !== pending.session || pending.metadata.sequence !== pending.sequence) {
                throw new Error("shared execution chunk metadata does not match its packet");
              }
            }
          } else if (offset !== pending.finalOffset) {
            throw new Error("shared execution chunk repeats a completed packet");
          }
          if (pending.json !== null) {
            if (apply(pending.json, pending.metadata) === false) break;
            this.#pending = null;
          }
        } else {
          if (this.#pending !== null) throw new Error("shared execution packet interrupted an incomplete chunk sequence");
          if (apply(item.json, item.metadata) === false) break;
        }
      } catch (error) {
        this.#pending = null;
        this.#release(item.slot, item.stateIndex);
        throw error;
      }
      this.#release(item.slot, item.stateIndex);
      consumed++;
    }
    // This counts freed physical slots, including partial chunks. A partial
    // packet has never reached apply(), let alone produced a displayed receipt.
    return consumed;
  }

  #read(slot, stateIndex) {
    const length = Atomics.load(this.#header, slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1);
    if (length <= 0 || length > this.#slotCapacity) throw new Error(`shared execution mailbox slot ${slot} has invalid length ${length}`);
    const offset = slot * this.#slotCapacity;
    if (length >= 4 && new DataView(this.#bytes.buffer, this.#bytes.byteOffset + offset, 4).getUint32(0, true) === CHUNK_MAGIC) {
      if (length <= CHUNK_HEADER_BYTES) throw new Error("shared execution chunk has an invalid header length");
      const view = new DataView(this.#bytes.buffer, this.#bytes.byteOffset + offset, CHUNK_HEADER_BYTES);
      const total = view.getUint32(4, true), partOffset = view.getUint32(8, true);
      const metadata = { session: view.getFloat64(16, true), sequence: view.getFloat64(24, true) };
      const payload = this.#bytes.subarray(offset + CHUNK_HEADER_BYTES, offset + length);
      if (view.getUint32(12, true) !== CHUNK_VERSION || total <= this.#slotCapacity ||
          total > MAX_SHARED_EXECUTION_DELTA_BYTES || partOffset >= total || payload.byteLength > total - partOffset ||
          !Number.isSafeInteger(metadata.session) || metadata.session < 0 ||
          !Number.isSafeInteger(metadata.sequence) || metadata.sequence < 0) {
        throw new Error("shared execution chunk has invalid framing");
      }
      return { slot, stateIndex, metadata, payload, chunk: { total, offset: partOffset } };
    }
    // Browsers cannot decode a SharedArrayBuffer-backed view directly.
    const json = decoder.decode(this.#bytes.slice(offset, offset + length));
    return { slot, stateIndex, json, metadata: executionDeltaMetadata(json) };
  }

  close() { this.#closed = true; this.#pending = null; }

  #release(slot, stateIndex) {
    Atomics.store(this.#header, slot === 0 ? SLOT_LENGTH_0 : SLOT_LENGTH_1, 0);
    Atomics.store(this.#header, stateIndex, SLOT_FREE);
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
    addMessageListener(port, (message) => this.#handleMessage(message));
    port.start?.();
  }

  canSend() {
    return this.#inFlight < this.#maxInFlight;
  }

  send(json, validatedMetadata = null) {
    const metadata = metadataForSend(json, validatedMetadata);
    if (this.#inFlight >= this.#maxInFlight) {
      this.#backpressure += 1;
      return false;
    }
    const payload = encoder.encode(json);
    // TextEncoder normally returns an exact, standalone buffer. Transfer that
    // allocation directly; retain the fallback for nonstandard typed-array views.
    const buffer = payload.byteOffset === 0 && payload.byteLength === payload.buffer.byteLength
      ? payload.buffer
      : payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
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
  #pending = [];
  #draining = false;

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
    addMessageListener(port, (message) => this.#handleMessage(message));
    port.start?.();
  }

  drain() {
    if (this.#draining) {
      return 0;
    }
    this.#draining = true;
    let consumed = 0;
    try {
      while (this.#pending.length > 0) {
        const item = this.#pending[0];
        if (this.#apply(item.json, item.metadata) === false) {
          break;
        }
        this.#pending.shift();
        this.#port.postMessage({
          type: "execution_ack",
          session: item.metadata.session,
          sequence: item.metadata.sequence,
        });
        this.#onWritable?.();
        consumed += 1;
      }
    } finally {
      this.#draining = false;
    }
    return consumed;
  }

  pendingCount() {
    return this.#pending.length;
  }

  #handleMessage(message) {
    if (!isRecord(message) || message.type !== "execution_delta") {
      return;
    }
    const item = decodeTransferableExecutionDelta(message);
    this.#pending.push(item);
    this.drain();
  }
}

function addMessageListener(port, handleMessage) {
  if (typeof port.addEventListener === "function") {
    port.addEventListener("message", (event) => handleMessage(event.data));
    return;
  }
  if (typeof port.on === "function") {
    port.on("message", handleMessage);
    return;
  }
  throw new TypeError("MessagePort-like object cannot receive messages");
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
