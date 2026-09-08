import { spawn } from "node:child_process";

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 1_000_000;
const DEFAULT_KILL_GRACE_MS = 1_000;
const SAFE_ENV_KEYS = ["HOME", "LANG", "LC_ALL", "PATH", "TMPDIR"];

export function buildScrubbedEnvironment(extra = {}) {
  if (extra === null || typeof extra !== "object" || Array.isArray(extra)) {
    throw new TypeError("extra environment must be an object");
  }
  const environment = {};
  for (const key of SAFE_ENV_KEYS) {
    const value = process.env[key];
    if (typeof value === "string" && value.length > 0) environment[key] = value;
  }
  for (const [key, value] of Object.entries(extra)) {
    if (!/^[A-Z_][A-Z0-9_]*$/.test(key)) {
      throw new TypeError(`invalid environment key ${key}`);
    }
    if (typeof value !== "string") {
      throw new TypeError(`environment value for ${key} must be a string`);
    }
    environment[key] = value;
  }
  return Object.freeze(environment);
}

function validatePositiveInteger(name, value) {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
    throw new RangeError(`${name} must be a positive bounded integer`);
  }
}

function terminateProcessTree(child, signal) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform !== "win32" && child.pid) {
    try {
      process.kill(-child.pid, signal);
      return;
    } catch (error) {
      if (error?.code !== "ESRCH") throw error;
    }
  }
  try {
    child.kill(signal);
  } catch (error) {
    if (error?.code !== "ESRCH") throw error;
  }
}

export async function runBoundedChild({
  command,
  args = [],
  cwd,
  env = {},
  timeoutMs = DEFAULT_TIMEOUT_MS,
  maxOutputBytes = DEFAULT_MAX_OUTPUT_BYTES,
  killGraceMs = DEFAULT_KILL_GRACE_MS,
}) {
  if (typeof command !== "string" || command.length === 0) {
    throw new TypeError("command must be a non-empty string");
  }
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string")) {
    throw new TypeError("args must be an array of strings");
  }
  if (typeof cwd !== "string" || cwd.length === 0) {
    throw new TypeError("cwd must be a non-empty string");
  }
  validatePositiveInteger("timeoutMs", timeoutMs);
  validatePositiveInteger("maxOutputBytes", maxOutputBytes);
  validatePositiveInteger("killGraceMs", killGraceMs);

  const startedAt = Date.now();
  const child = spawn(command, args, {
    cwd,
    env: buildScrubbedEnvironment(env),
    detached: process.platform !== "win32",
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });

  let stdout = Buffer.alloc(0);
  let stderr = Buffer.alloc(0);
  let terminationReason = null;
  let forcedKill = false;
  let graceTimer = null;

  const append = (streamName, chunk) => {
    const current = streamName === "stdout" ? stdout : stderr;
    const nextLength = current.length + chunk.length;
    if (nextLength > maxOutputBytes) {
      terminationReason ??= "output_limit";
      terminateProcessTree(child, "SIGTERM");
      if (graceTimer === null) {
        graceTimer = setTimeout(() => {
          if (child.exitCode === null && child.signalCode === null) {
            forcedKill = true;
            terminateProcessTree(child, "SIGKILL");
          }
        }, killGraceMs);
      }
      return;
    }
    if (streamName === "stdout") stdout = Buffer.concat([stdout, chunk]);
    else stderr = Buffer.concat([stderr, chunk]);
  };

  child.stdout.on("data", (chunk) => append("stdout", chunk));
  child.stderr.on("data", (chunk) => append("stderr", chunk));

  const timeout = setTimeout(() => {
    terminationReason ??= "timeout";
    terminateProcessTree(child, "SIGTERM");
    graceTimer = setTimeout(() => {
      if (child.exitCode === null && child.signalCode === null) {
        forcedKill = true;
        terminateProcessTree(child, "SIGKILL");
      }
    }, killGraceMs);
  }, timeoutMs);

  try {
    const result = await new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("close", (exitCode, signal) => resolve({ exitCode, signal }));
    });
    return Object.freeze({
      ...result,
      terminationReason,
      forcedKill,
      durationMs: Date.now() - startedAt,
      stdout: stdout.toString("utf8"),
      stderr: stderr.toString("utf8"),
    });
  } finally {
    clearTimeout(timeout);
    if (graceTimer !== null) clearTimeout(graceTimer);
  }
}
