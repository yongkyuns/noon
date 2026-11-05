import { createHash } from "node:crypto";
import { writeFile } from "node:fs/promises";
import path from "node:path";

const TARGETS = Object.freeze({
  authoring: /(?:^|\/)python-worker\.js(?:[?#]|$)/,
  render: /(?:^|\/)execution-render-worker\.js(?:[?#]|$)/,
});

export class WorkerCpuProfileCapture {
  #cdp;
  #pending = new Map();
  #profiles = new Map();
  #nextMessageId = 1;
  #detached = new Set();
  #lifecycle = "new";

  constructor(cdp) {
    this.#cdp = cdp;
    cdp.on("Target.receivedMessageFromTarget", ({ sessionId, message }) => {
      const response = JSON.parse(message);
      const pending = this.#pending.get(response.id);
      if (!pending) return;
      this.#pending.delete(response.id);
      clearTimeout(pending.timer);
      if (response.error) pending.reject(new Error(response.error.message ?? JSON.stringify(response.error)));
      else pending.resolve(response.result ?? {});
    });
    cdp.on("Target.detachedFromTarget", ({ sessionId }) => this.#detached.add(sessionId));
  }

  async attach() {
    if (this.#lifecycle !== "new") throw new Error(`cannot attach profiler in ${this.#lifecycle} state`);
    this.#lifecycle = "attaching";
    await this.#cdp.send("Target.setDiscoverTargets", { discover: true });
    const { targetInfos } = await this.#cdp.send("Target.getTargets");
    for (const [owner, pattern] of Object.entries(TARGETS)) {
      const matches = targetInfos.filter(target => target.type === "worker" && pattern.test(target.url));
      if (matches.length !== 1) {
        throw new Error(`expected one live ${owner} worker target, found ${matches.length}`);
      }
      const target = matches[0];
      const { sessionId } = await this.#cdp.send("Target.attachToTarget", {
        targetId: target.targetId, flatten: false,
      });
      this.#profiles.set(owner, { targetId: target.targetId, sessionId, url: target.url });
      await this.#send(sessionId, "Profiler.enable");
      await this.#send(sessionId, "Profiler.setSamplingInterval", { interval: 1_000 });
    }
    this.#lifecycle = "attached";
  }

  async start(boundary) {
    if (this.#lifecycle !== "attached") throw new Error(`cannot start profiler in ${this.#lifecycle} state`);
    if (!Number.isSafeInteger(boundary?.runGeneration) || !Number.isSafeInteger(boundary?.session) ||
        !Number.isFinite(boundary?.time) || !Number.isFinite(boundary?.rendererAt) ||
        typeof boundary?.exampleId !== "string") {
      throw new Error("worker CPU profile start is missing a valid pre-Run renderer anchor");
    }
    this.#lifecycle = "starting";
    for (const entry of this.#profiles.values()) {
      if (this.#detached.has(entry.sessionId)) throw new Error(`worker detached before profile start: ${entry.url}`);
    }
    const requestedAtMs = Date.now();
    await Promise.all([...this.#profiles.values()].map(async entry => {
      entry.startRequestedAtMs = Date.now();
      await this.#send(entry.sessionId, "Profiler.start");
      entry.startResponseAtMs = Date.now();
    }));
    this.startBoundary = { requestedAtMs, responseAtMs: Date.now(), sample: boundary,
      workers: Object.fromEntries([...this.#profiles].map(([owner, entry]) => [owner, {
        requestedAtMs: entry.startRequestedAtMs, responseAtMs: entry.startResponseAtMs,
      }])) };
    this.#lifecycle = "started";
  }

  async stop(boundary, artifactDir) {
    if (this.#lifecycle !== "started") throw new Error(`cannot stop profiler in ${this.#lifecycle} state`);
    if (!Number.isSafeInteger(boundary?.runGeneration) ||
        boundary.runGeneration <= this.startBoundary?.sample?.runGeneration ||
        boundary?.exampleId !== this.startBoundary?.sample?.exampleId ||
        !Number.isSafeInteger(boundary?.session) || !Number.isFinite(boundary?.time) ||
        !Number.isFinite(boundary?.rendererAt)) {
      throw new Error("worker CPU profile end must identify the new completed run and renderer receipt");
    }
    this.#lifecycle = "stopping";
    const requestedAtMs = Date.now();
    const { targetInfos } = await this.#cdp.send("Target.getTargets");
    const liveTargetIds = new Set(targetInfos.map(target => target.targetId));
    const profiles = {};
    const profileStopTimes = {};
    for (const [owner, entry] of this.#profiles) {
      if (this.#detached.has(entry.sessionId) || !liveTargetIds.has(entry.targetId)) {
        throw new Error(`profiled worker target was replaced or detached: ${entry.url}`);
      }
      const stopRequestedAtMs = Date.now();
      const { profile } = await this.#send(entry.sessionId, "Profiler.stop");
      profileStopTimes[owner] = {
        stopRequestedAtMs,
        stopResponseAtMs: Date.now(),
        startRequestedAtMs: entry.startRequestedAtMs,
        startResponseAtMs: entry.startResponseAtMs,
      };
      const bytes = Buffer.from(JSON.stringify(profile));
      const filename = `worker-${owner}.cpuprofile`;
      await writeFile(path.join(artifactDir, filename), bytes);
      profiles[owner] = {
        targetId: entry.targetId,
        targetUrl: entry.url,
        file: filename,
        sha256: createHash("sha256").update(bytes).digest("hex"),
        nodeCount: profile.nodes?.length ?? 0,
        sampleCount: profile.samples?.length ?? 0,
        startTime: profile.startTime,
        endTime: profile.endTime,
      };
      if (!(profiles[owner].nodeCount > 0 && profiles[owner].sampleCount > 0)) {
        throw new Error(`empty CPU profile for ${owner} worker`);
      }
    }
    const result = {
      status: "captured",
      scope: "diagnostic-only whole warm pass from pre-Run start through looped source completion; sampling perturbs scheduling and is not an FPS measurement",
      samplingIntervalMicros: 1_000,
      boundary: { start: this.startBoundary, stop: { requestedAtMs, responseAtMs: Date.now(), sample: boundary,
        workers: profileStopTimes } },
      profiles,
    };
    this.#lifecycle = "stopped";
    return result;
  }

  async #send(sessionId, method, params = {}) {
    const id = this.#nextMessageId++;
    let timer;
    const response = new Promise((resolve, reject) => {
      timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`timed out waiting for ${method} on worker session ${sessionId}`));
      }, 5_000);
      this.#pending.set(id, { resolve, reject, timer });
    });
    try {
      await this.#cdp.send("Target.sendMessageToTarget", {
        sessionId,
        message: JSON.stringify({ id, method, params }),
      });
    } catch (error) {
      clearTimeout(timer);
      this.#pending.delete(id);
      throw error;
    }
    return response;
  }
}
