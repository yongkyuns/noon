import { spawn } from "node:child_process";
import { addAbortListener } from "node:events";
import { performance } from "node:perf_hooks";

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 1_000_000;
const DEFAULT_KILL_GRACE_MS = 1_000;
const DEFAULT_CLEANUP_TIMEOUT_MS = 1_000;
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
    if (value.includes("\0")) throw new TypeError(`environment value for ${key} contains NUL`);
    environment[key] = value;
  }
  return Object.freeze(environment);
}

function validatePositiveInteger(name, value) {
  if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
    throw new RangeError(`${name} must be a positive bounded integer`);
  }
}

// Decoding malformed/truncated UTF-8 inserts replacement characters, which can
// expand a raw-byte prefix. Bound the returned UTF-8 text as well, without
// splitting a Unicode scalar. Only re-encode when replacement expansion needs it.
function boundedDiagnostic(buffer, maxBytes, truncated) {
  const text = buffer.toString("utf8");
  if (Buffer.byteLength(text, "utf8") <= maxBytes) return { text, truncated };
  const encoded = Buffer.from(text, "utf8");
  let end = maxBytes;
  while ((encoded[end] & 0xc0) === 0x80) end--;
  return { text: encoded.toString("utf8", 0, end), truncated: true };
}

/**
 * Linux process-group lifecycle only, NOT a sandbox or orphan reaper. Descendants
 * that change session/group can escape. The caller must translate close,
 * disconnect and graceful shutdown into an AbortSignal and await this promise;
 * abrupt supervisor death is outside this boundary.
 *
 * timeoutMs bounds execution before cleanup. Cleanup adds at most killGraceMs
 * plus cleanupTimeoutMs (subject to event-loop/OS scheduling). maxOutputBytes
 * caps both captured raw bytes and returned UTF-8 diagnostic bytes per stream.
 * Raw overflow requests termination; decode-time truncation only sets the
 * corresponding stdoutTruncated/stderrTruncated flag, as does raw truncation.
 * These are not JSON/wire payload limits. Cancellation returns diagnostics
 * rather than rejecting.
 *
 * cleanup.outcome is deliberately evidence-qualified: group_absent means ESRCH
 * was observed, sigkill_sent means only that the kernel accepted the group kill
 * (zombies may remain), and incomplete means signaling or stdio drainage failed.
 * None of these outcomes proves containment of escaped-session descendants.
 */
export async function runBoundedChild({
  command,
  args = [],
  cwd,
  env = {},
  signal,
  timeoutMs = DEFAULT_TIMEOUT_MS,
  maxOutputBytes = DEFAULT_MAX_OUTPUT_BYTES,
  killGraceMs = DEFAULT_KILL_GRACE_MS,
  cleanupTimeoutMs = DEFAULT_CLEANUP_TIMEOUT_MS,
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
  validatePositiveInteger("cleanupTimeoutMs", cleanupTimeoutMs);
  if (signal !== undefined && !(signal instanceof AbortSignal)) {
    throw new TypeError("signal must be an AbortSignal");
  }
  const environment = buildScrubbedEnvironment(env);
  // No Windows direct-child fallback masquerading as descendant ownership.
  if (process.platform !== "linux") {
    throw new Error("process-group supervisor currently supports Linux only");
  }

  const startedAt = performance.now();
  let stdout = Buffer.alloc(0);
  let stderr = Buffer.alloc(0);
  let stdoutTruncated = false;
  let stderrTruncated = false;
  let exitCode = null;
  let exitSignal = null;
  let terminationReason = null;
  let forcedKill = false;
  let leaderExited = false;
  let stdioClosed = false;
  const errors = [];
  const result = (outcome) => {
    const out = boundedDiagnostic(stdout, maxOutputBytes, stdoutTruncated);
    const err = boundedDiagnostic(stderr, maxOutputBytes, stderrTruncated);
    return Object.freeze({
      exitCode, signal: exitSignal, terminationReason, forcedKill,
      durationMs: performance.now() - startedAt,
      stdout: out.text, stderr: err.text,
      stdoutTruncated: out.truncated, stderrTruncated: err.truncated,
      cleanup: Object.freeze({
        scope: "linux_process_group", outcome, leaderExited, stdioClosed,
        errors: Object.freeze(errors.map((error) => Object.freeze(error))),
      }),
    });
  };
  if (signal?.aborted) {
    terminationReason = "canceled";
    stdioClosed = true;
    return result("not_started");
  }

  const child = spawn(command, args, {
    cwd, env: environment, detached: true, stdio: ["ignore", "pipe", "pipe"],
  });
  const timers = new Set();
  const after = (ms) => new Promise((resolve) => {
    const timer = setTimeout(() => { timers.delete(timer); resolve(); }, ms);
    timers.add(timer);
  });
  let requestStop;
  const stopped = new Promise((resolve) => {
    requestStop = (reason) => { terminationReason ??= reason; resolve(); };
  });
  let processError;
  const leaderDone = new Promise((resolve) => {
    child.once("exit", (code, receivedSignal) => {
      leaderExited = true;
      exitCode = code;
      exitSignal = receivedSignal;
      resolve();
    });
    child.once("error", (error) => { processError = error; resolve(); });
  });
  const closed = new Promise((resolve) => {
    child.once("close", () => { stdioClosed = true; resolve(true); });
  });
  const append = (streamName, chunk) => {
    const current = streamName === "stdout" ? stdout : stderr;
    const remaining = maxOutputBytes - current.length;
    // Continue draining during cleanup, but never re-copy a saturated buffer.
    if (remaining > 0) {
      const next = Buffer.concat([current, chunk.subarray(0, remaining)]);
      if (streamName === "stdout") stdout = next;
      else stderr = next;
    }
    if (chunk.length > remaining) {
      if (streamName === "stdout") stdoutTruncated = true;
      else stderrTruncated = true;
      requestStop("output_limit");
    }
  };
  child.stdout.on("data", (chunk) => append("stdout", chunk));
  child.stderr.on("data", (chunk) => append("stderr", chunk));
  for (const stream of [child.stdout, child.stderr]) {
    stream.once("error", (error) => {
      errors.push({ operation: "stdio", code: error.code ?? "UNKNOWN" });
      requestStop("io_error");
    });
  }

  // Group identity, never the direct child's exitCode, controls termination.
  const signalGroup = (requestedSignal) => {
    if (!child.pid) return "not_started";
    try {
      process.kill(-child.pid, requestedSignal);
      return "sent";
    } catch (error) {
      if (error.code === "ESRCH") return "group_absent";
      errors.push({ operation: requestedSignal, code: error.code ?? "UNKNOWN" });
      return "incomplete";
    }
  };
  let abortSubscription;
  const deadline = setTimeout(() => requestStop("timeout"), timeoutMs);
  try {
    if (signal) abortSubscription = addAbortListener(signal, () => requestStop("canceled"));
    // Exit and close are distinct: inherited pipes may outlive the leader,
    // while independent pipes may close with a descendant still executing.
    await Promise.race([leaderDone, stopped]);
    clearTimeout(deadline);
    let outcome = signalGroup("SIGTERM");
    if (outcome === "sent" || outcome === "incomplete") {
      terminationReason ??= "descendant_cleanup";
      // One non-resettable grace window. Neither a flood, another cancellation,
      // nor direct-child close is allowed to cancel or postpone escalation.
      await after(killGraceMs);
      outcome = signalGroup("SIGKILL");
      if (outcome === "sent") {
        forcedKill = true;
        outcome = "sigkill_sent";
      }
    }
    const drained = await Promise.race([closed, after(cleanupTimeoutMs).then(() => false)]);
    if (!drained || errors.length > 0) {
      if (!drained) terminationReason ??= "cleanup_timeout";
      outcome = "incomplete";
    }
    const report = result(outcome);
    if (processError) {
      processError.cleanup = report.cleanup;
      throw processError;
    }
    return report;
  } finally {
    clearTimeout(deadline);
    for (const timer of timers) clearTimeout(timer);
    abortSubscription?.[Symbol.dispose]();
    // An escaped pipe owner or unkillable child must not retain local resources
    // forever. Failure is explicit above; unref/destroy is not a containment win.
    child.stdout.destroy();
    child.stderr.destroy();
    child.unref();
  }
}
