import { execFile, spawn } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const short = (value, limit = 300) => typeof value === "string" ? value.slice(0, limit) : null;
const nativeLogCapacity = 128 * 1024;
// A verbose startup can fill 128 KiB before the failing event. Drain the finite
// PID/time-qualified query and keep its tail instead of killing `log` on overflow.
// This retains the same bounded amount of evidence without losing later events.
export function readNativeProcessLog(args, {
  launch = request => spawn("/usr/bin/log", request, { stdio: ["ignore", "pipe", "pipe"] }),
  timeoutMs = 5000,
} = {}) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 5000) {
    throw new RangeError("invalid native log deadline");
  }
  return new Promise((resolve, reject) => {
    let stdout = Buffer.alloc(0), stderr = Buffer.alloc(0), observedBytes = 0;
    let failure = null;
    const tail = (previous, chunk, capacity) => {
      const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
      if (bytes.length >= capacity) return Buffer.from(bytes.subarray(bytes.length - capacity));
      return Buffer.concat([previous.subarray(Math.max(0, previous.length + bytes.length - capacity)), bytes]);
    };
    const child = launch(args);
    child.stdout.on("data", chunk => {
      observedBytes += chunk.length;
      stdout = tail(stdout, chunk, nativeLogCapacity);
    });
    child.stderr.on("data", chunk => { stderr = tail(stderr, chunk, 4096); });
    child.on("error", error => { failure ??= error; });
    const timer = setTimeout(() => {
      failure ??= Object.assign(new Error(`native log query timed out after ${timeoutMs} ms`), { code: "ETIMEDOUT" });
      child.kill("SIGKILL");
    }, timeoutMs);
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      const result = { stdout, stderr, observedBytes, truncated: observedBytes > stdout.length };
      if (code !== 0 || signal !== null) {
        failure ??= new Error(`native log query exited with code ${code}, signal ${signal}`);
      }
      if (failure) reject(Object.assign(failure, result));
      else resolve(result);
    });
  });
}

/** Bounded evidence from the test-owned file server; never restart or retry it. */
export function captureChildProcessDiagnostics(child, { maxOutputChars = 16_000 } = {}) {
  if (!Number.isSafeInteger(maxOutputChars) || maxOutputChars < 1 || maxOutputChars > 64_000) {
    throw new RangeError("invalid child-process output capacity");
  }
  let stdoutTail = "", stderrTail = "", error = null;
  const subscriptions = [];
  const on = (target, event, listener) => {
    if (target == null) return;
    target.on(event, listener);
    subscriptions.push([target, event, listener]);
  };
  on(child.stdout, "data", chunk => { stdoutTail = (stdoutTail + chunk.toString()).slice(-maxOutputChars); });
  on(child.stderr, "data", chunk => { stderrTail = (stderrTail + chunk.toString()).slice(-maxOutputChars); });
  on(child, "error", cause => { error = short(String(cause)); });
  return Object.freeze({ snapshot: () => ({
    pid: child.pid ?? null, exitCode: child.exitCode ?? null,
    signalCode: child.signalCode ?? null, error, stdoutTail, stderrTail,
  }), dispose: () => {
    for (const [target, event, listener] of subscriptions) target.off(event, listener);
    subscriptions.length = 0;
  } });
}

/** Host-side observations, independent of a crashed page's JS execution context. */
export function createBrowserFailureTrace({ now = () => Date.now(), maxEvents = 32 } = {}) {
  if (!Number.isSafeInteger(maxEvents) || maxEvents < 1 || maxEvents > 128) {
    throw new RangeError("invalid browser failure trace capacity");
  }
  let phase = "setup";
  const events = [];
  // Python/Emscripten can print a fatal explanation to stdout or a warning
  // before an opaque WASM abort. Keep a separate finite tail so verbose output
  // cannot evict lifecycle evidence. Never resolve console arguments or RPCs.
  const consoleMessages = [];
  let consoleMessageCount = 0;
  const record = (kind, facts = {}) => {
    events.push({ atMs: now(), phase, kind, ...facts });
    if (events.length > maxEvents) events.shift();
  };
  const setPhase = (name, facts = {}) => {
    if (typeof name !== "string" || !name || name.length > 80) throw new TypeError("invalid phase");
    phase = name;
    record("phase", facts);
  };
  const attach = (browser, context, page) => {
    const subscriptions = new Set();
    const on = (target, event, listener) => {
      target.on(event, listener);
      const subscription = [target, event, listener];
      subscriptions.add(subscription);
      return () => { target.off(event, listener); subscriptions.delete(subscription); };
    };
    on(browser, "disconnected", () => record("browser-disconnected"));
    on(context, "close", () => record("context-close"));
    on(page, "crash", () => record("page-crash"));
    on(page, "close", () => record("page-close"));
    on(page, "console", message => {
      const text = message.text();
      const location = message.location();
      consoleMessageCount++;
      consoleMessages.push({
        atMs: now(), phase, type: short(message.type(), 32),
        text: short(text, 2048), textTruncated: text.length > 2048,
        url: short(location.url, 500),
        lineNumber: location.lineNumber, columnNumber: location.columnNumber,
      });
      if (consoleMessages.length > maxEvents) consoleMessages.shift();
    });
    on(context, "requestfailed", request => record("request-failed", {
      url: short(request.url(), 500), error: short(request.failure()?.errorText),
    }));
    on(context, "response", response => {
      if (response.status() >= 400) record("http-error", {
        url: short(response.url(), 500), status: response.status(),
      });
    });
    on(page, "worker", worker => {
      const url = short(worker.url(), 500);
      record("worker-created", { url });
      const detach = on(worker, "close", () => { record("worker-close", { url }); detach(); });
    });
    on(page, "framenavigated", frame => {
      try {
        if (frame === page.mainFrame()) record("main-frame-navigation", { url: short(frame.url()) });
      } catch { record("main-frame-navigation-unavailable"); }
    });
    // Gallery cases reuse Chromium/Firefox browsers; detach only this trace's
    // listeners so completed cases neither retain pages nor accumulate events.
    return () => {
      for (const [target, event, listener] of subscriptions) target.off(event, listener);
      subscriptions.clear();
    };
  };
  return Object.freeze({ setPhase, record, attach, snapshot: () => ({
    phase, events: events.map(event => ({ ...event })),
    consoleMessageCount, consoleMessagesOmitted: consoleMessageCount - consoleMessages.length,
    consoleMessages: consoleMessages.map(message => ({ ...message })),
  }) });
}

async function observePage(page, callback, argument, timeoutMs) {
  let timer;
  try {
    return await Promise.race([
      Promise.resolve().then(() => page.evaluate(callback, argument)),
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`page observation timed out after ${timeoutMs} ms`)), timeoutMs);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

// Failure-only observations: preserve the DOM even when the render worker cannot
// answer. Each protocol call has its own deadline; neither retries nor starts a
// deferred execution, and metrics never consume optional profiling samples.
/** Obtain the precise attempted page state, never substitute a prior scene. */
export async function captureAttemptedPage(page, { timeoutMs = 2000 } = {}) {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 5000) {
    throw new RangeError("invalid page observation deadline");
  }
  if (page === null) return { state: "unavailable", reason: "page not created" };
  try {
    const observed = await observePage(page, () => {
      const controls = document.querySelector(".playback-controls");
      const toggle = controls?.querySelector(".playback-toggle");
      return {
        url: location.href,
        visibilityState: document.visibilityState ?? null,
        documentHasFocus: document.hasFocus?.() ?? null,
        runGeneration: window.__noonExampleGallery?.generationDiagnostics?.runGeneration ?? null,
        runDisabled: document.querySelector("#replace-scene")?.disabled ?? null,
        selectedExampleId: document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId ?? null,
        patchState: document.querySelector("#patch-status")?.dataset.state ?? null,
        patchText: (document.querySelector("#patch-status")?.value ?? "").slice(0, 300),
        runtimeStartup: document.querySelector("#status")?.dataset.runtimeStartup ?? null,
        runtimeState: document.querySelector("#status")?.dataset.state ?? null,
        executionMode: document.querySelector("#status")?.dataset.executionMode ?? null,
        playbackControls: controls == null ? null : {
          data: { ...controls.dataset },
          toggle: toggle == null ? null : {
            ariaLabel: toggle.getAttribute("aria-label"),
            disabled: toggle.disabled,
            text: toggle.textContent,
          },
        },
        // This is the test's bounded click/command trace, independent of engine
        // RPCs. A failed playback query must not erase the attempted interaction.
        playbackTrace: window.__matrixPlaybackTrace?.events?.slice(-32) ?? [],
      };
    }, undefined, timeoutMs);
    let executionMetrics = { state: "unavailable", reason: "execution not initialized" };
    if (observed.executionMode) {
      try {
        executionMetrics = await observePage(page, async expected => {
          const gallery = window.__noonExampleGallery;
          const sameRun = () => gallery === window.__noonExampleGallery && location.href === expected.url &&
            (gallery?.generationDiagnostics?.runGeneration ?? null) === expected.runGeneration &&
            (document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId ?? null) === expected.selectedExampleId &&
            (document.querySelector("#status")?.dataset.executionMode ?? null) === expected.executionMode;
          if (!sameRun()) return { state: "unavailable", reason: "scene changed during observation" };
          if (typeof gallery?.executionMetrics !== "function") {
            return { state: "unavailable", reason: "execution metrics unavailable" };
          }
          const response = await gallery.executionMetrics();
          if (!sameRun()) return { state: "unavailable", reason: "scene changed during observation" };
          const metrics = response?.metrics;
          if (metrics === null || typeof metrics !== "object" || Array.isArray(metrics)) {
            return { state: "unavailable", reason: "execution metrics unavailable" };
          }
          // Fixed scalar projection: no scene rows, resource payloads or trace arrays.
          const progress = {};
          for (const key of ["ready", "mode", "backend", "time", "objectCount", "presentedFrames",
            "presentedSession", "lastFrameTimestamp", "bufferedDeltas", "needsPresent",
            "lastDeltaApplyMs", "lastRendererCallMs", "sampledAtMs", "performanceTimeOriginMs",
            "gpuGeneration", "rendererRebuilds", "transitionMode", "resourceBundlePending"]) {
            const value = metrics[key];
            if (value === null || typeof value === "boolean" ||
                (typeof value === "number" && Number.isFinite(value)) ||
                (typeof value === "string" && value.length <= 128)) progress[key] = value;
          }
          return { state: "captured", observed: progress };
        }, observed, timeoutMs);
      } catch (error) {
        executionMetrics = { state: "unavailable", reason: short(String(error)) };
      }
    }
    return { state: "captured", observed, executionMetrics };
  } catch (error) {
    let pageClosed = null;
    try { pageClosed = page.isClosed(); } catch { /* browser is already gone */ }
    return { state: "unavailable", pageClosed, reason: short(String(error)) };
  }
}

/** Match the current Playwright WebKit installation, never arbitrary process names. */
export function parsePlaywrightWebKitProcesses(output, executablePath) {
  if (typeof output !== "string" || typeof executablePath !== "string" || !path.isAbsolute(executablePath)) {
    throw new TypeError("expected ps output and absolute Playwright executable path");
  }
  const prefix = path.dirname(executablePath) + path.sep;
  const result = [];
  const seen = new Set();
  for (const line of output.split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(.+)$/);
    if (!match) continue;
    const pid = Number(match[1]);
    const executable = match[2];
    if (!Number.isSafeInteger(pid) || pid < 1 || seen.has(pid) || !executable.startsWith(prefix)) continue;
    seen.add(pid);
    result.push({ pid, executable: short(executable, 512) });
    if (result.length === 32) break;
  }
  return result;
}

/** Failure-only native context for known PIDs, including a vanished WebContent process. */
export async function captureNativeProcessLog({ pids, startedAtMs, endedAtMs, artifacts,
  prefix, readNativeLog = readNativeProcessLog }) {
  const owned = [...new Set(pids.filter(pid => Number.isSafeInteger(pid) && pid > 0))];
  if (!Number.isFinite(startedAtMs) || !Number.isFinite(endedAtMs) || endedAtMs < startedAtMs) {
    return { reason: "invalid native log window" };
  }
  if (owned.length === 0) return { reason: "no observed WebKit process IDs" };
  const result = { pids: owned.slice(0, 32), startedAtMs, endedAtMs };
  result.omittedPidCount = owned.length - result.pids.length;
  if (endedAtMs - startedAtMs > 180_000) {
    return { ...result, reason: "native log window exceeds 180 seconds" };
  }
  // Use whole-second Unix bounds accepted by log. Keep the actual query
  // window separate from the original case's millisecond timestamps.
  result.queriedStartAtMs = Math.floor(startedAtMs / 1000) * 1000;
  result.queriedEndAtMs = Math.ceil(endedAtMs / 1000) * 1000;
  const args = ["show", "--style", "ndjson", "--timezone", "UTC", "--info", "--debug",
    "--start", `@${result.queriedStartAtMs / 1000}`,
    "--end", `@${result.queriedEndAtMs / 1000}`, "--predicate",
    result.pids.map(pid => `processIdentifier == ${pid}`).join(" OR ")];
  let stdout = Buffer.alloc(0);
  try {
    const response = await readNativeLog(args);
    stdout = response.stdout;
    result.observedBytes = response.observedBytes ?? stdout.length;
    result.truncated = response.truncated === true;
    if (response.stderr?.length) result.stderr = short(String(response.stderr));
  }
  catch (error) {
    stdout = error.stdout ?? stdout;
    result.error = short(String(error));
    result.stderr = short(String(error.stderr ?? ""));
    result.observedBytes = error.observedBytes ?? stdout.length;
    result.truncated = error.truncated === true || error.code === "ERR_CHILD_PROCESS_STDIO_MAXBUFFER";
  }
  try {
    const bytes = Buffer.isBuffer(stdout) ? stdout : Buffer.from(stdout ?? "");
    const kept = bytes.subarray(Math.max(0, bytes.length - nativeLogCapacity));
    result.truncated ||= bytes.length > kept.length;
    result.retained = "tail";
    const file = `${prefix}-native.log`;
    await writeFile(path.join(artifacts, file), kept);
    result.file = file;
    result.bytes = kept.length;
  } catch (error) { result.writeError = short(String(error)); }
  return result;
}

/** Collect only failure evidence, never retry a failed browser cell. */
export function createNativeWebKitEvidence({ enabled = false, executablePath, artifacts,
  prefix = "matrix-failure", startedAtMs = Date.now(), now = () => Date.now(),
  readProcessTable = async () => (await promisify(execFile)("/bin/ps", ["-axo", "pid=,comm="], {
    timeout: 5000, maxBuffer: 1024 * 1024,
  })).stdout,
  collect,
  collectLog = captureNativeProcessLog,
} = {}) {
  if (enabled && typeof collect !== "function") throw new TypeError("native crash report collector required");
  const snapshots = [];
  const observed = new Set();
  const observe = async (stage) => {
    if (!enabled) return;
    try {
      const processes = parsePlaywrightWebKitProcesses(await readProcessTable(), executablePath);
      for (const { pid } of processes) observed.add(pid);
      snapshots.push({ stage, atMs: now(), processes });
    } catch (error) {
      snapshots.push({ stage, atMs: now(), error: short(String(error)) });
    }
  };
  const failure = async () => {
    if (!enabled) return null;
    await observe("after-failure");
    const request = {
      directories: [path.join(homedir(), "Library/Logs/DiagnosticReports"), "/Library/Logs/DiagnosticReports"],
      pids: [...observed], startedAtMs, endedAtMs: now(), artifacts, prefix,
    };
    let result;
    try {
      result = await collect(request);
    } catch (error) {
      result = { reports: [], errors: [{ message: short(String(error)) }] };
    }
    let processLog;
    try { processLog = await collectLog(request); }
    catch (error) { processLog = { error: short(String(error)) }; }
    return { ...result, processLog };
  };
  return Object.freeze({ observe, failure, snapshots: () => snapshots.map(item => ({ ...item, processes: item.processes?.map(x => ({ ...x })) })) });
}
