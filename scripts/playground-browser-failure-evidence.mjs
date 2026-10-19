import { execFile } from "node:child_process";
import { homedir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const short = (value, limit = 300) => typeof value === "string" ? value.slice(0, limit) : null;

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
  }) });
}

/** Obtain the precise attempted page state, never substitute a prior scene. */
export async function captureAttemptedPage(page) {
  if (page === null) return { state: "unavailable", reason: "page not created" };
  try {
    const observed = await page.evaluate(() => {
      const controls = document.querySelector(".playback-controls");
      const toggle = controls?.querySelector(".playback-toggle");
      return {
        url: location.href,
        runDisabled: document.querySelector("#replace-scene")?.disabled ?? null,
        selectedExampleId: document.querySelector(".example-card[aria-selected='true']")?.dataset.exampleId ?? null,
        patchState: document.querySelector("#patch-status")?.dataset.state ?? null,
        patchText: (document.querySelector("#patch-status")?.value ?? "").slice(0, 300),
        runtimeStartup: document.querySelector("#status")?.dataset.runtimeStartup ?? null,
        runtimeState: document.querySelector("#status")?.dataset.state ?? null,
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
    });
    return { state: "captured", observed };
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

/** Collect only failure evidence, never retry a failed browser cell. */
export function createNativeWebKitEvidence({ enabled = false, executablePath, artifacts,
  startedAtMs = Date.now(), now = () => Date.now(),
  readProcessTable = async () => (await promisify(execFile)("/bin/ps", ["-axo", "pid=,comm="], {
    timeout: 5000, maxBuffer: 1024 * 1024,
  })).stdout,
  collect,
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
    try {
      return await collect({
        directories: [path.join(homedir(), "Library/Logs/DiagnosticReports"), "/Library/Logs/DiagnosticReports"],
        pids: [...observed], startedAtMs, endedAtMs: now(), artifacts, prefix: "matrix-failure",
      });
    } catch (error) {
      return { reports: [], errors: [{ message: short(String(error)) }] };
    }
  };
  return Object.freeze({ observe, failure, snapshots: () => snapshots.map(item => ({ ...item, processes: item.processes?.map(x => ({ ...x })) })) });
}
