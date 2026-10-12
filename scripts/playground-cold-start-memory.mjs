import { execFile } from "node:child_process";
import { performance } from "node:perf_hooks";
import { promisify } from "node:util";

const MAX_SAMPLES = 2_400;
const PS_TIMEOUT_MS = 2_000;
const execFileAsync = promisify(execFile);

export function parsePsProcessTable(text) {
  const processes = new Map();
  for (const line of String(text).split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)$/);
    if (match) {
      const [, pid, ppid, rssKb] = match;
      processes.set(Number(pid), { ppid: Number(ppid), rssBytes: Number(rssKb) * 1024 });
    }
  }
  return processes;
}

export function aggregateProcessTreeRss(rootPid, processes) {
  if (!Number.isSafeInteger(rootPid) || rootPid <= 0) throw new TypeError("root PID must be positive");
  if (!(processes instanceof Map)) throw new TypeError("process table must be a Map");
  if (!processes.has(rootPid)) return null;

  const children = new Map();
  for (const [pid, process] of processes) {
    const siblings = children.get(process.ppid) ?? [];
    siblings.push(pid);
    children.set(process.ppid, siblings);
  }
  const included = new Set();
  const pending = [rootPid];
  while (pending.length > 0) {
    const pid = pending.pop();
    if (included.has(pid)) continue;
    included.add(pid);
    pending.push(...(children.get(pid) ?? []));
  }
  let rssBytes = 0;
  for (const pid of included) {
    const process = processes.get(pid);
    if (process) rssBytes += process.rssBytes;
  }
  return { rssBytes, processCount: included.size };
}

export function createProcessTreeRssSampler(rootPid, { intervalMs = 250, maxSamples = MAX_SAMPLES } = {}) {
  if (!Number.isSafeInteger(rootPid) || rootPid <= 0) throw new TypeError("root PID must be positive");
  if (!Number.isSafeInteger(intervalMs) || intervalMs < 50) throw new TypeError("sample interval must be at least 50 ms");
  if (!Number.isSafeInteger(maxSamples) || maxSamples < 1 || maxSamples > MAX_SAMPLES) {
    throw new TypeError(`sample count must be between 1 and ${MAX_SAMPLES}`);
  }

  const startedAt = performance.now();
  const samples = [];
  let timer = null;
  let pending = null;
  let stopped = false;
  let attempts = 0;
  let samplingError = null;

  async function takeSample() {
    if (stopped || pending !== null || attempts >= maxSamples) return pending;
    attempts += 1;
    pending = (async () => {
      const table = await readProcessTable();
      const aggregate = aggregateProcessTreeRss(rootPid, table);
      if (aggregate !== null) {
        samples.push({ atMs: performance.now() - startedAt, ...aggregate });
      }
    })().catch((error) => {
      samplingError = String(error);
    }).finally(() => {
      pending = null;
      if (attempts >= maxSamples && timer !== null) {
        clearInterval(timer);
        timer = null;
      }
    });
    return pending;
  }

  return {
    start() {
      if (timer !== null || stopped) throw new Error("RSS sampler can only be started once");
      timer = setInterval(() => { void takeSample(); }, intervalMs);
      void takeSample();
    },
    async stop() {
      if (timer !== null) clearInterval(timer);
      timer = null;
      if (pending !== null) await pending;
      if (!stopped && attempts < maxSamples) await takeSample();
      stopped = true;
      const peak = samples.reduce((best, sample) =>
        best === null || sample.rssBytes > best.rssBytes ? sample : best, null);
      return {
        enabled: true,
        metric: "sampled-chromium-process-tree-rss",
        intervalMs,
        maxSamples,
        attemptCount: attempts,
        sampleCount: samples.length,
        truncated: attempts >= maxSamples,
        samplingError,
        peakSampledRssBytes: peak?.rssBytes ?? null,
        peakSampleAtMs: peak?.atMs ?? null,
        processCountAtPeak: peak?.processCount ?? null,
        sampledWindowMs: samples.length > 1 ? samples.at(-1).atMs - samples[0].atMs : 0,
        samples,
        caveat: "Sampled aggregate RSS of the Chromium process tree; shared pages may be counted more than once and GPU allocations outside process RSS are not included. This is not total physical memory use or an instantaneous peak.",
      };
    },
  };
}

function readProcessTable() {
  return execFileAsync("ps", ["-axo", "pid=,ppid=,rss="], {
    timeout: PS_TIMEOUT_MS,
    maxBuffer: 4 * 1024 * 1024,
  }).then(({ stdout }) => parsePsProcessTable(stdout));
}
