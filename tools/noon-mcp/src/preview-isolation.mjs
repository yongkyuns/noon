import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { addAbortListener } from "node:events";
import { homedir } from "node:os";
import path from "node:path";
import { readFile, realpath, stat } from "node:fs/promises";

export const PREVIEW_RUNTIME_SCHEMA_VERSION = 1;
export const DEFAULT_PREVIEW_LIMITS = Object.freeze({
  memoryBytes: 1_073_741_824,
  cpuCount: 2,
  pids: 128,
  shmBytes: 268_435_456,
  workBytes: 268_435_456,
  tmpBytes: 134_217_728,
});

const IMAGE_ID = /^sha256:[0-9a-f]{64}$/;
const CONTAINER_ID = /^[0-9a-f]{12,64}$/;
const CONTAINER_NAME = /^noon-preview-[0-9a-f-]{36}$/;
const MAX_DOCKER_OUTPUT = 256 * 1024;

function boundedInt(name, value, min, max) {
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new RangeError(`${name} must be an integer in [${min}, ${max}]`);
  }
  return value;
}

function boundedNumber(name, value, min, max) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < min || value > max) {
    throw new RangeError(`${name} must be finite in [${min}, ${max}]`);
  }
  return value;
}

export function normalizePreviewLimits(input = {}) {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    throw new TypeError("preview limits must be an object");
  }
  const limits = { ...DEFAULT_PREVIEW_LIMITS, ...input };
  return Object.freeze({
    memoryBytes: boundedInt("memoryBytes", limits.memoryBytes, 268_435_456, 8_589_934_592),
    cpuCount: boundedNumber("cpuCount", limits.cpuCount, 0.25, 8),
    pids: boundedInt("pids", limits.pids, 32, 512),
    shmBytes: boundedInt("shmBytes", limits.shmBytes, 67_108_864, 1_073_741_824),
    workBytes: boundedInt("workBytes", limits.workBytes, 67_108_864, 1_073_741_824),
    tmpBytes: boundedInt("tmpBytes", limits.tmpBytes, 33_554_432, 536_870_912),
  });
}

function safeHostPath(name, value) {
  if (typeof value !== "string" || !path.isAbsolute(value) || value.includes("\0") || value.includes(",")) {
    throw new TypeError(`${name} must be an absolute host path without NUL or comma`);
  }
  return value;
}

async function regularRealPath(name, value, { directory = false } = {}) {
  const canonical = await realpath(safeHostPath(name, value));
  const metadata = await stat(canonical);
  if (directory ? !metadata.isDirectory() : !metadata.isFile()) {
    throw new TypeError(`${name} must resolve to a ${directory ? "directory" : "regular file"}`);
  }
  return canonical;
}

export async function loadPreviewRuntimeConfig({
  configPath = path.join(homedir(), ".cache", "noon-preview", "runtime.json"),
  repoRoot,
  dockerExecutable = "docker",
  limits,
} = {}) {
  if (typeof dockerExecutable !== "string" || dockerExecutable.length === 0 || dockerExecutable.includes("\0")) {
    throw new TypeError("dockerExecutable must be non-empty text");
  }
  const resolvedConfig = await regularRealPath("preview runtime config", configPath);
  let parsed;
  try {
    parsed = JSON.parse(await readFile(resolvedConfig, "utf8"));
  } catch (error) {
    throw new Error(`invalid preview runtime config: ${error.message}`);
  }
  if (parsed?.schemaVersion !== PREVIEW_RUNTIME_SCHEMA_VERSION || !IMAGE_ID.test(parsed?.imageId ?? "")) {
    throw new Error("preview runtime config has unsupported schema or image identity");
  }
  const checkout = await regularRealPath("repoRoot", repoRoot, { directory: true });
  const webRoot = await regularRealPath("web root", path.join(checkout, "web"), { directory: true });
  const toolingRoot = await regularRealPath("MCP tooling root", path.join(checkout, "tools", "noon-mcp"), { directory: true });
  const seccompProfile = await regularRealPath("seccomp profile", parsed.seccompProfile);
  return Object.freeze({
    dockerExecutable,
    imageId: parsed.imageId,
    seccompProfile,
    repoRoot: checkout,
    webRoot,
    toolingRoot,
    limits: normalizePreviewLimits(limits),
  });
}

export function buildDockerCreateArgs(config, command, { containerName } = {}) {
  if (!config || typeof config !== "object" || !IMAGE_ID.test(config.imageId ?? "")) {
    throw new TypeError("Docker preview config requires a content-addressed image ID");
  }
  if (!Array.isArray(command) || command.length === 0 || command.some((part) => typeof part !== "string" || part.includes("\0"))) {
    throw new TypeError("container command must be a non-empty string array");
  }
  const limits = normalizePreviewLimits(config.limits);
  const webRoot = safeHostPath("webRoot", config.webRoot);
  const toolingRoot = safeHostPath("toolingRoot", config.toolingRoot);
  const seccomp = safeHostPath("seccompProfile", config.seccompProfile);
  const cpu = String(limits.cpuCount);
  if (containerName !== undefined && !CONTAINER_NAME.test(containerName)) {
    throw new TypeError("containerName must be an owned Noon preview name");
  }
  return Object.freeze([
    "create",
    ...(containerName ? [`--name=${containerName}`] : []),
    "--rm",
    "--init",
    "--interactive",
    "--read-only",
    "--network=none",
    "--ipc=private",
    "--pid=private",
    "--user=pwuser",
    `--memory=${limits.memoryBytes}`,
    `--memory-swap=${limits.memoryBytes}`,
    `--cpus=${cpu}`,
    `--pids-limit=${limits.pids}`,
    `--shm-size=${limits.shmBytes}`,
    "--cap-drop=ALL",
    "--security-opt=no-new-privileges=true",
    `--security-opt=seccomp=${seccomp}`,
    `--tmpfs=/work:rw,nosuid,nodev,mode=1777,size=${limits.workBytes}`,
    `--tmpfs=/tmp:rw,nosuid,nodev,mode=1777,size=${limits.tmpBytes}`,
    `--mount=type=bind,src=${webRoot},dst=/noon/web,readonly`,
    `--mount=type=bind,src=${toolingRoot},dst=/noon/tools/noon-mcp,readonly`,
    "--workdir=/work",
    "--env=HOME=/work/home",
    "--env=TMPDIR=/tmp",
    "--env=NOON_WEB_ROOT=/noon/web",
    "--label=com.noon.preview=true",
    config.imageId,
    ...command,
  ]);
}

export function validateDockerInspection(inspect, config) {
  if (!inspect || typeof inspect !== "object") throw new Error("Docker inspect returned no container record");
  const host = inspect.HostConfig ?? {};
  const container = inspect.Config ?? {};
  const limits = normalizePreviewLimits(config.limits);
  const mounts = Array.isArray(inspect.Mounts) ? inspect.Mounts : [];
  const mount = (destination) => mounts.find((entry) => entry.Destination === destination);
  const security = Array.isArray(host.SecurityOpt) ? host.SecurityOpt : [];
  const capDrop = Array.isArray(host.CapDrop) ? host.CapDrop.map((value) => String(value).toUpperCase()) : [];
  const failures = [];
  if (inspect.Image !== config.imageId) failures.push("container image identity mismatch");
  if (host.NetworkMode !== "none") failures.push("network must be none");
  if (host.IpcMode !== "private") failures.push("IPC namespace must be private");
  if (host.PidMode !== "private") failures.push("PID namespace must be private");
  if (host.ReadonlyRootfs !== true) failures.push("root filesystem must be read-only");
  if (host.Privileged === true) failures.push("privileged mode forbidden");
  if (container.User !== "pwuser") failures.push("container must run as pwuser");
  if (Number(host.Memory) !== limits.memoryBytes) failures.push("memory limit mismatch");
  if (Number(host.MemorySwap) !== limits.memoryBytes) failures.push("swap limit mismatch");
  if (Number(host.NanoCpus) !== Math.round(limits.cpuCount * 1e9)) failures.push("CPU limit mismatch");
  if (Number(host.PidsLimit) !== limits.pids) failures.push("PID limit mismatch");
  if (Number(host.ShmSize) !== limits.shmBytes) failures.push("shared-memory limit mismatch");
  if (!capDrop.includes("ALL")) failures.push("all Linux capabilities must be dropped");
  if (!security.some((value) => value.startsWith("no-new-privileges"))) failures.push("no-new-privileges required");
  if (!security.some((value) => value.startsWith("seccomp="))) failures.push("explicit seccomp profile required");
  const tmpfs = host.Tmpfs ?? {};
  if (typeof tmpfs["/work"] !== "string" || !tmpfs["/work"].includes(`size=${limits.workBytes}`)) failures.push("bounded /work tmpfs required");
  if (typeof tmpfs["/tmp"] !== "string" || !tmpfs["/tmp"].includes(`size=${limits.tmpBytes}`)) failures.push("bounded /tmp tmpfs required");
  for (const [destination, expectedSource] of [
    ["/noon/web", config.webRoot],
    ["/noon/tools/noon-mcp", config.toolingRoot],
  ]) {
    const entry = mount(destination);
    if (!entry || entry.RW !== false || entry.Type !== "bind" || entry.Source !== expectedSource) {
      failures.push(`${destination} must be the expected read-only bind mount`);
    }
  }
  if (failures.length > 0) throw new Error(`Docker isolation verification failed: ${failures.join("; ")}`);
  return true;
}

function captureProcess(executable, args, { signal, maxBytes = MAX_DOCKER_OUTPUT, timeoutMs = 15_000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: ["ignore", "pipe", "pipe"] });
    let stdout = Buffer.alloc(0);
    let stderr = Buffer.alloc(0);
    let overflow = false;
    let settled = false;
    let abortSubscription;
    const timer = setTimeout(() => child.kill("SIGKILL"), timeoutMs);
    const append = (current, chunk) => {
      const remaining = maxBytes - current.length;
      if (remaining <= 0) { overflow = true; return current; }
      if (chunk.length > remaining) overflow = true;
      return Buffer.concat([current, chunk.subarray(0, Math.max(0, remaining))]);
    };
    const finish = (callback) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      abortSubscription?.[Symbol.dispose]();
      callback();
    };
    child.stdout.on("data", (chunk) => { stdout = append(stdout, chunk); });
    child.stderr.on("data", (chunk) => { stderr = append(stderr, chunk); });
    child.once("error", (error) => finish(() => reject(error)));
    child.once("close", (code, receivedSignal) => finish(() => {
      const detail = stderr.toString("utf8").trim().slice(0, 1200);
      if (overflow) return reject(new Error("Docker command output exceeded diagnostic limit"));
      if (code !== 0) return reject(new Error(`Docker command failed (${code ?? receivedSignal}): ${detail}`));
      resolve(stdout.toString("utf8").trim());
    }));
    if (signal) abortSubscription = addAbortListener(signal, () => child.kill("SIGKILL"));
  });
}

function ownedContainerSelector(value) {
  return CONTAINER_ID.test(value ?? "") || CONTAINER_NAME.test(value ?? "");
}

function missingContainer(error) {
  return /No such (?:container|object)/i.test(String(error?.message ?? error));
}

async function removeContainer(config, selector) {
  if (!ownedContainerSelector(selector)) {
    return Object.freeze({ outcome: "invalid_selector", removed: false });
  }
  try {
    await captureProcess(config.dockerExecutable, ["rm", "--force", selector], { timeoutMs: 10_000 });
    return Object.freeze({ outcome: "removed", removed: true });
  } catch (error) {
    if (missingContainer(error)) return Object.freeze({ outcome: "already_absent", removed: true });
    return Object.freeze({ outcome: "failed", removed: false, error: String(error.message ?? error).slice(0, 1200) });
  }
}

export class DockerIsolatedProcess {
  #config;
  #containerId;
  #attached;
  #closePromise = null;
  #abortSubscription;
  #stderr = Buffer.alloc(0);
  #stderrTruncated = false;
  #cleanupError = null;
  #exited;

  static async launch(config, command, { signal } = {}) {
    if (signal !== undefined && !(signal instanceof AbortSignal)) throw new TypeError("signal must be an AbortSignal");
    if (signal?.aborted) throw new Error("preview launch canceled before container creation");
    const containerName = `noon-preview-${randomUUID()}`;
    const createArgs = buildDockerCreateArgs(config, command, { containerName });
    let id;
    try {
      id = await captureProcess(config.dockerExecutable, createArgs, { timeoutMs: 15_000 });
    } catch (error) {
      const cleanup = await removeContainer(config, containerName);
      if (!cleanup.removed) error.cleanup = cleanup;
      throw error;
    }
    if (!CONTAINER_ID.test(id)) {
      const cleanup = await removeContainer(config, containerName);
      const error = new Error("Docker returned an invalid container identity");
      if (!cleanup.removed) error.cleanup = cleanup;
      throw error;
    }
    if (signal?.aborted) {
      const cleanup = await removeContainer(config, id);
      const error = new Error("preview launch canceled after container creation");
      if (!cleanup.removed) error.cleanup = cleanup;
      throw error;
    }
    try {
      const inspectionText = await captureProcess(config.dockerExecutable, ["inspect", id], { signal, timeoutMs: 10_000 });
      const inspection = JSON.parse(inspectionText)?.[0];
      validateDockerInspection(inspection, config);
      const attached = spawn(config.dockerExecutable, ["start", "--attach", "--interactive", id], {
        stdio: ["pipe", "pipe", "pipe"],
      });
      return new DockerIsolatedProcess(config, id, attached, signal);
    } catch (error) {
      const cleanup = await removeContainer(config, id);
      if (!cleanup.removed) error.cleanup = cleanup;
      throw error;
    }
  }

  constructor(config, containerId, attached, signal) {
    this.#config = config;
    this.#containerId = containerId;
    this.#attached = attached;
    this.#exited = new Promise((resolve) => {
      let settled = false;
      const finish = (value) => {
        if (settled) return;
        settled = true;
        resolve(Object.freeze(value));
      };
      attached.once("close", (code, receivedSignal) => finish({ code, signal: receivedSignal, error: null }));
      attached.once("error", (error) => {
        const message = String(error?.message ?? error).slice(0, 1200);
        this.#cleanupError ??= message;
        finish({ code: null, signal: null, error: message });
      });
    });
    attached.stderr.on("data", (chunk) => {
      const remaining = MAX_DOCKER_OUTPUT - this.#stderr.length;
      if (remaining <= 0) { this.#stderrTruncated = true; return; }
      if (chunk.length > remaining) this.#stderrTruncated = true;
      this.#stderr = Buffer.concat([this.#stderr, chunk.subarray(0, remaining)]);
    });
    if (signal) this.#abortSubscription = addAbortListener(signal, () => {
      void this.close("preview operation canceled").catch((error) => {
        this.#cleanupError = String(error.message ?? error).slice(0, 1200);
      });
    });
  }

  get containerId() { return this.#containerId; }
  get stdin() { return this.#attached.stdin; }
  get stdout() { return this.#attached.stdout; }
  get exited() { return this.#exited; }
  get diagnostics() {
    return Object.freeze({ stderr: this.#stderr.toString("utf8"), stderrTruncated: this.#stderrTruncated, cleanupError: this.#cleanupError });
  }

  close(reason = "preview container closed") {
    if (this.#closePromise) return this.#closePromise;
    if (typeof reason !== "string" || reason.trim() === "") {
      return Promise.reject(new TypeError("close reason must be non-empty"));
    }
    this.#abortSubscription?.[Symbol.dispose]();
    this.#closePromise = (async () => {
      try { this.#attached.stdin.end(); } catch {}
      const cleanup = await removeContainer(this.#config, this.#containerId);
      try { await Promise.race([this.#exited, new Promise((resolve) => setTimeout(resolve, 1_500))]); } catch {}
      if (this.#attached.exitCode === null && this.#attached.signalCode === null) this.#attached.kill("SIGKILL");
      if (!cleanup.removed) {
        this.#cleanupError = cleanup.error ?? "preview container cleanup failed";
        throw new Error(this.#cleanupError);
      }
      return Object.freeze({ closed: true, containerId: this.#containerId, cleanup });
    })();
    return this.#closePromise;
  }
}
