import { createHash } from "node:crypto";
import { addAbortListener } from "node:events";

import { DockerIsolatedProcess } from "./preview-isolation.mjs";

const MAX_LINE_BYTES = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES = 1_000_000;
const MAX_TIME_SECONDS = 600;
const MAX_PNG_BYTES = 4 * 1024 * 1024;

function validateSource(value) {
  if (typeof value !== "string" || value.trim() === "" || value.includes("\0")) {
    throw new TypeError("preview source must be non-empty text without NUL");
  }
  if (Buffer.byteLength(value, "utf8") > MAX_SOURCE_BYTES) {
    throw new RangeError("preview source exceeds byte limit");
  }
  return value;
}

function validateTime(value, { positive = false } = {}) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > MAX_TIME_SECONDS || (positive && value === 0)) {
    throw new RangeError(`preview time must be ${positive ? "positive" : "non-negative"}, finite, and <= ${MAX_TIME_SECONDS}`);
  }
  return value;
}

function requireSignal(signal) {
  if (signal !== undefined && !(signal instanceof AbortSignal)) {
    throw new TypeError("signal must be an AbortSignal");
  }
  return signal;
}

function operationSignal(lifecycle, signal) {
  requireSignal(signal);
  return signal ? AbortSignal.any([lifecycle, signal]) : lifecycle;
}

function abortMessage(signal, fallback = "preview operation canceled") {
  const reason = signal?.reason;
  if (reason instanceof Error && reason.message) return reason.message;
  return typeof reason === "string" && reason.trim() ? reason : fallback;
}

function decodedImage(record) {
  if (!record || record.mimeType !== "image/png" || !Number.isSafeInteger(record.width) || !Number.isSafeInteger(record.height) ||
      !Number.isSafeInteger(record.encodedBytes) || record.encodedBytes <= 8 || record.encodedBytes > MAX_PNG_BYTES || typeof record.base64 !== "string") {
    throw new Error("preview worker returned invalid image metadata");
  }
  const data = Buffer.from(record.base64, "base64");
  if (data.length !== record.encodedBytes || data[0] !== 137 || data[1] !== 80 || data[2] !== 78 || data[3] !== 71) {
    throw new Error("preview worker returned invalid PNG content");
  }
  return Object.freeze({
    mimeType: "image/png",
    width: record.width,
    height: record.height,
    encodedBytes: data.length,
    sha256: createHash("sha256").update(data).digest("hex"),
    data,
  });
}

export class JsonLineRpcClient {
  #stdin;
  #stdout;
  #buffer = Buffer.alloc(0);
  #pending = new Map();
  #nextId = 1;
  #closed = false;
  #fatal = null;

  constructor({ stdin, stdout }) {
    if (!stdin || typeof stdin.write !== "function" || !stdout || typeof stdout.on !== "function") {
      throw new TypeError("preview RPC requires writable stdin and readable stdout");
    }
    this.#stdin = stdin;
    this.#stdout = stdout;
    stdout.on("data", (chunk) => this.#ingest(Buffer.from(chunk)));
    stdout.once("end", () => this.#fail(new Error("preview worker stdout ended")));
    stdout.once("error", (error) => this.#fail(error));
    stdin.once?.("error", (error) => this.#fail(error));
  }

  async request(payload, { signal, timeoutMs = 35_000 } = {}) {
    if (this.#closed) throw this.#fatal ?? new Error("preview RPC is closed");
    requireSignal(signal);
    if (signal?.aborted) throw new Error(abortMessage(signal, "preview request canceled"));
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > 120_000) {
      throw new RangeError("preview RPC timeout must be a positive bounded integer");
    }
    if (this.#pending.size !== 0) throw new Error("preview RPC operation already in progress");
    const id = this.#nextId++;
    if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
      throw new TypeError("preview RPC payload must be an object");
    }
    const line = `${JSON.stringify({ ...payload, id })}\n`;
    if (Buffer.byteLength(line, "utf8") > MAX_LINE_BYTES) {
      throw new RangeError("preview request exceeds protocol byte limit");
    }
    let abortSubscription;
    let timer;
    const response = new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      timer = setTimeout(() => {
        if (!this.#pending.delete(id)) return;
        const error = new Error("preview worker request timed out");
        reject(error);
        this.#fail(error);
      }, timeoutMs);
      if (signal) abortSubscription = addAbortListener(signal, () => {
        if (!this.#pending.delete(id)) return;
        const error = new Error(abortMessage(signal, "preview request canceled"));
        reject(error);
        this.#fail(error);
      });
    });
    try {
      if (!this.#stdin.write(line, "utf8")) {
        await new Promise((resolve, reject) => {
          const onError = (error) => { cleanup(); reject(error); };
          const onDrain = () => { cleanup(); resolve(); };
          const cleanup = () => {
            this.#stdin.off?.("error", onError);
            this.#stdin.off?.("drain", onDrain);
          };
          this.#stdin.once("error", onError);
          this.#stdin.once("drain", onDrain);
        });
      }
      const record = await response;
      if (record.ok !== true) {
        throw new Error(typeof record.error === "string" ? record.error : "preview worker rejected request");
      }
      return record;
    } finally {
      clearTimeout(timer);
      abortSubscription?.[Symbol.dispose]();
      this.#pending.delete(id);
    }
  }

  close(reason = "preview RPC closed") {
    if (this.#closed) return false;
    this.#closed = true;
    this.#fatal = reason instanceof Error ? reason : new Error(String(reason));
    for (const { reject } of this.#pending.values()) reject(this.#fatal);
    this.#pending.clear();
    try { this.#stdin.end(); } catch {}
    return true;
  }

  #ingest(chunk) {
    if (this.#closed) return;
    this.#buffer = Buffer.concat([this.#buffer, chunk]);
    if (this.#buffer.length > MAX_LINE_BYTES) {
      this.#fail(new Error("preview response exceeds protocol byte limit"));
      return;
    }
    while (true) {
      const newline = this.#buffer.indexOf(0x0a);
      if (newline < 0) break;
      const line = this.#buffer.subarray(0, newline);
      this.#buffer = this.#buffer.subarray(newline + 1);
      if (line.length === 0) continue;
      let record;
      try { record = JSON.parse(line.toString("utf8")); }
      catch {
        this.#fail(new Error("preview worker returned invalid JSON"));
        return;
      }
      const pending = this.#pending.get(record.id);
      if (!pending) {
        this.#fail(new Error("preview worker returned an unknown request id"));
        return;
      }
      this.#pending.delete(record.id);
      pending.resolve(record);
    }
  }

  #fail(error) {
    if (this.#closed) return;
    this.close(error);
  }
}

export class DockerPreviewSession {
  #config;
  #process = null;
  #launchPromise = null;
  #rpc = null;
  #snapshot = Object.freeze({ state: "new", frame: null, image: null });
  #latestImage = null;
  #lastRequestedTime = 0;
  #state = "new";
  #closePromise = null;
  #lifecycle = new AbortController();

  constructor(config) {
    if (!config || typeof config !== "object") {
      throw new TypeError("Docker preview session requires isolation config");
    }
    this.#config = config;
  }

  get snapshot() { return this.#snapshot; }
  get latestImage() {
    if (!this.#latestImage) return null;
    const { data, ...descriptor } = this.#latestImage;
    return Object.freeze({ ...descriptor, data: Buffer.from(data) });
  }

  async open(source, { loopDurationSeconds = 4, signal } = {}) {
    if (this.#state !== "new") throw new Error("Docker preview session opens only once");
    validateSource(source);
    validateTime(loopDurationSeconds, { positive: true });
    const activeSignal = operationSignal(this.#lifecycle.signal, signal);
    if (activeSignal.aborted) throw new Error(abortMessage(activeSignal));
    this.#state = "opening";
    try {
      this.#launchPromise = DockerIsolatedProcess.launch(this.#config,
        ["node", "/noon/tools/noon-mcp/src/preview-worker.mjs"], { signal: activeSignal });
      const process = await this.#launchPromise;
      this.#process = process;
      this.#launchPromise = null;
      if (activeSignal.aborted || this.#state !== "opening") throw new Error(abortMessage(activeSignal));
      this.#rpc = new JsonLineRpcClient({ stdin: process.stdin, stdout: process.stdout });
      const response = await this.#rpc.request({ op: "open", source, loopDurationSeconds }, { signal: activeSignal });
      if (activeSignal.aborted || this.#state !== "opening") throw new Error(abortMessage(activeSignal));
      this.#accept(response);
      this.#state = "ready";
      return this.#result();
    } catch (error) {
      this.#launchPromise = null;
      if (!this.#closePromise) this.#state = "failed";
      await this.close(`preview open failed: ${String(error?.message ?? error)}`).catch(() => {});
      throw error;
    }
  }

  async sample(timeSeconds, { signal } = {}) {
    if (this.#state !== "ready") throw new Error("Docker preview session is not ready");
    const time = validateTime(timeSeconds);
    if (time < this.#lastRequestedTime) throw new RangeError("preview playback cannot move backwards");
    const activeSignal = operationSignal(this.#lifecycle.signal, signal);
    try {
      const response = await this.#rpc.request({ op: "sample", time }, { signal: activeSignal });
      if (activeSignal.aborted || this.#state !== "ready") throw new Error(abortMessage(activeSignal));
      this.#accept(response);
      this.#lastRequestedTime = time;
      return this.#result();
    } catch (error) {
      await this.close(`preview sample failed: ${String(error?.message ?? error)}`).catch(() => {});
      throw error;
    }
  }

  async inspect({ signal } = {}) {
    if (this.#state !== "ready") throw new Error("Docker preview session is not ready");
    const activeSignal = operationSignal(this.#lifecycle.signal, signal);
    try {
      const response = await this.#rpc.request({ op: "inspect" }, { signal: activeSignal, timeoutMs: 10_000 });
      if (!response.snapshot || typeof response.snapshot !== "object") {
        throw new Error("preview worker returned invalid snapshot");
      }
      this.#snapshot = Object.freeze({ ...response.snapshot, image: this.#imageDescriptor() });
      return this.#snapshot;
    } catch (error) {
      await this.close(`preview inspect failed: ${String(error?.message ?? error)}`).catch(() => {});
      throw error;
    }
  }

  close(reason = "Docker preview session closed") {
    if (this.#closePromise) return this.#closePromise;
    if (typeof reason !== "string" || reason.trim() === "") {
      return Promise.reject(new TypeError("preview close reason must be non-empty"));
    }
    this.#state = "closing";
    this.#lifecycle.abort(reason);
    this.#closePromise = (async () => {
      // Container removal is the authoritative cancellation boundary. Do not
      // enqueue a graceful browser command behind potentially stuck user code.
      this.#rpc?.close(reason);
      let process = this.#process;
      if (!process && this.#launchPromise) {
        try { process = await this.#launchPromise; }
        catch { /* launch owns failed-create cleanup by its generated container name */ }
      }
      let cleanup = null;
      let cleanupError = null;
      if (process) {
        try { cleanup = await process.close(reason); }
        catch (error) { cleanupError = error instanceof Error ? error : new Error(String(error)); }
      }
      this.#process = null;
      this.#launchPromise = null;
      this.#state = "closed";
      this.#snapshot = Object.freeze({
        ...this.#snapshot,
        state: "closed",
        cleanup,
        error: cleanupError?.message ?? null,
      });
      if (cleanupError) throw cleanupError;
      return this.#snapshot;
    })();
    return this.#closePromise;
  }

  #accept(response) {
    if (!response.snapshot || typeof response.snapshot !== "object") {
      throw new Error("preview worker returned invalid snapshot");
    }
    this.#latestImage = decodedImage(response.image);
    this.#snapshot = Object.freeze({ ...response.snapshot, image: this.#imageDescriptor() });
    const requested = Number(response.snapshot?.frame?.requestedTime);
    if (Number.isFinite(requested)) this.#lastRequestedTime = requested;
  }

  #imageDescriptor() {
    if (!this.#latestImage) return null;
    const { data: _data, ...descriptor } = this.#latestImage;
    return Object.freeze(descriptor);
  }

  #result() {
    return Object.freeze({
      ...this.#snapshot,
      imageData: this.#latestImage ? Buffer.from(this.#latestImage.data) : null,
    });
  }
}

export function createDockerPreviewSessionFactory(config) {
  return () => new DockerPreviewSession(config);
}
