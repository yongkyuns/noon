// Paired, equal-work host-boundary measurements on the exact production packages.
// No wall-clock animation sleeps, dropped-case retries, threshold adaptation,
// per-frame Python instrumentation, or mutation of the runtime under test.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { profileSource, validateProfile, localEditProfileSource, validateLocalEditProfile } from "./python-host-profile.mjs";
import { qualifyProductMetrics } from "./paired-product-metrics.mjs";
import { diagnoseWorkerHistory, diagnoseControlledHistory } from "./python-host-perf-history.mjs";
import { stringifyEvidence } from "./python-host-report.mjs";
import { PERF_PROTOCOL as protocol, assertComparableArtifacts, pairedCost, performanceSource } from "./python-host-perf-protocol.mjs";

import { verifyProductArtifact } from "../.github/ci/product-artifact.mjs";

const roots = ["BASELINE", "CANDIDATE"].map(side => {
  assert.ok(process.env[`NOON_PERF_${side}_ROOT`], `missing ${side} source`);
  return path.resolve(process.env[`NOON_PERF_${side}_ROOT`]);
});
const output = path.join(roots[1], "browser-smoke-artifacts/product-gate/host-cost");
await mkdir(output, { recursive: true });
let identities = [], changedBuildInputs = [];
const cache = createPyodideResourceCache(await readFile(path.join(roots[1], "web/python-worker.source.js"), "utf8"));
const servers = [], contexts = [], pages = [], reports = [[], []], warmups = [], rows = [], failures = [];
const profiles = [[], []], diagnostics = [];
const localProfiles = [[], []], localDiagnostics = [];
let browser;
try {
  for (const [side, root] of roots.entries()) {
    const role = side === 0 ? "baseline" : "candidate";
    // The producer pair is immutable even on retries with a newer event/ref.
    // Revalidate the pair, bytes, inventory, lockfile, source and build recipe;
    // standalone invocation cannot trust an artifact's claimed identity.
    identities.push(await verifyProductArtifact(root, process.env, role));
  }
  changedBuildInputs = assertComparableArtifacts(identities);
  browser = await playwright.chromium.launch({ headless: true, args: browserArgs("webgl") });
  for (const root of roots) {
    const server = await serveRepository(root, 0, { crossOriginIsolated: true }); servers.push(server);
    const context = await browser.newContext({ viewport: { width: 320, height: 180 } }); contexts.push(context);
    await cache.install(context);
    const page = await context.newPage(); pages.push(page);
    page.on("console", message => {
      if (message.text().startsWith("NOON_PERF_REPORT ")) reports[pages.indexOf(page)].push(JSON.parse(message.text().slice(17)));
      if (message.text().startsWith("NOON_PERF_PROFILE ")) profiles[pages.indexOf(page)].push(JSON.parse(message.text().slice(18)));
      if (message.text().startsWith("NOON_PERF_LOCAL_PROFILE ")) localProfiles[pages.indexOf(page)]
        .push(JSON.parse(message.text().slice("NOON_PERF_LOCAL_PROFILE ".length)));
    });
    page.on("pageerror", error => failures.push({ kind: "pageerror", message: String(error) }));
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
    await page.evaluate(async () => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      window.perfAuthoring = new PythonAuthoringClient();
      await window.perfAuthoring.ready();
    });
  }
  const openScoredParticipant = async (side, label) => {
    const context = await browser.newContext({ viewport: { width: 320, height: 180 } });
    try {
      await cache.install(context);
      const page = await context.newPage();
      const observations = [];
      page.on("console", message => {
        if (message.text().startsWith("NOON_PERF_REPORT ")) {
          observations.push(JSON.parse(message.text().slice(17)));
        }
      });
      page.on("pageerror", error => failures.push({
        kind: "scored_worker_pageerror", side, label, message: String(error),
      }));
      await page.goto(`${servers[side].baseUrl}/web/execution-worker-smoke.html`);
      await page.evaluate(async () => {
        const { PythonAuthoringClient } = await import("./authoring-client.js");
        window.perfAuthoring = new PythonAuthoringClient();
        await window.perfAuthoring.ready();
      });
      return { page, reports: observations, close: () => context.close() };
    } catch (error) {
      await context.close();
      throw error;
    }
  };

  const measure = async (side, source, mode, workload, participant = null) => {
    const page = participant?.page ?? pages[side];
    const observations = participant?.reports ?? reports[side];
    const count = observations.length;
    let timer;
    const operation = page.evaluate(async ({ source, samples, sampleHz }) => {
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const authoring = window.perfAuthoring;
      const canvas = document.createElement("canvas"); canvas.width = 320; canvas.height = 180;
      document.body.replaceChildren(canvas);
      const errors = [];
      let resolveAttachment, rejectAttachment, attachment;
      const attached = new Promise((resolve, reject) => { resolveAttachment = resolve; rejectAttachment = reject; });
      attached.catch(() => {});
      const execution = new AuthoringExecutionClient(canvas, { onError(error) { errors.push(String(error)); rejectAttachment(error); } });
      const started = performance.now();
      const terminal = authoring.run(source, {}, { onSemanticContinuation(registration) {
        attachment = execution.startSemanticExecution(registration.semanticExecution, {
          authoringClient: authoring, transportMode: "transferable", pacing: "external_samples",
        });
        attachment.then(resolveAttachment, rejectAttachment);
      } }).then(value => ({ ok: true, value }), error => ({ ok: false, error: String(error) }));
      const failed = terminal.then(result => result.ok ? new Promise(() => {}) : Promise.reject(new Error(result.error)));
      failed.catch(() => {});
      const advanceMs = [];
      try {
        await Promise.race([attached, failed]);
        for (let i = 0; i <= samples; ++i) {
          const start = performance.now();
          const receipt = await Promise.race([execution.sampleToAuthoredTime(i / sampleHz, { stopAtSourceCompletion: true }), failed]);
          advanceMs.push(performance.now() - start);
          if (receipt.sourceCompleted) break;
        }
        const completed = await terminal;
        if (!completed.ok) throw new Error(completed.error);
        if (errors.length) throw new Error(errors.join("\n"));
        return { elapsedMs: performance.now() - started, advanceMs,
          duration: completed.value.duration, metrics: (await execution.metrics()).metrics };
      } finally {
        await attachment?.catch(() => {});
        execution.terminate(); canvas.remove();
      }
    }, { source, samples: protocol.samples, sampleHz: protocol.sampleHz });
    let result;
    try {
      result = await Promise.race([operation, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(`timed out: ${side}/${workload}/${mode}`)), 120000);
      })]);
    } finally { clearTimeout(timer); }
    assert.equal(observations.length, count + 1, "missing Python timing observation");
    const report = observations.at(-1);
    assert.equal(report.mode, mode); assert.equal(report.workload, workload);
    assert.equal(report.coroutine, mode !== "jspi", "wrong source execution path");
    assert.equal(result.duration, 1.25, "changed authored extent");
    assert.equal(result.metrics.backend, "WebGL2", "changed renderer");
    assert.equal(result.metrics.objectCount, protocol.objects, "changed object work");
    assert.equal(result.metrics.geometryCacheMisses, 0, "steady-state geometry rebuilt");
    assert.ok(result.metrics.presentedFrames > 1 && result.metrics.instancesDrawn > 0, "no rendering");
    return { ...report, ...result };
  };
  for (const workload of protocol.workloads) {
    for (const mode of protocol.modes) {
      const source = performanceSource(mode, workload);
      const sourceSha = createHash("sha256").update(source).digest("hex");
      await writeFile(path.join(output, `${workload}-${mode}.py`), source);
      // Every scored pair uses two new workers. Each exact scored worker first
      // receives the same unscored source warmup, then one scored observation.
      // This preserves independent pair ratios without turning qualification
      // into a cold-worker benchmark. Browser/package startup and warmup timings
      // never enter the paired cost calculation; pair order remains alternating.
      const pairs = [];
      for (let pair = 0; pair < protocol.pairs; ++pair) {
        const participants = await Promise.all([0, 1].map(side =>
          openScoredParticipant(side, `pair-${workload}-${mode}-${pair + 1}`)));
        const values = [];
        try {
          const order = pair % 2 ? [1, 0] : [0, 1];
          for (let warm = 0; warm < protocol.scoredWorkerWarmups; ++warm) {
            for (const side of order) {
              warmups.push({ side, workload, mode, pair: pair + 1, warmup: warm + 1,
                workerLifetime: "fresh-per-pair-warmed",
                result: await measure(side, source, mode, workload, participants[side]) });
            }
          }
          for (const side of order) {
            values[side] = await measure(side, source, mode, workload, participants[side]);
          }
        } finally {
          await Promise.all(participants.map(participant => participant.close()));
        }
        assert.equal(values[1].callback_calls, values[0].callback_calls, "different Python callback work");
        assert.ok(values[0].center.every((v, i) => Math.abs(v - values[1].center[i]) < 2e-5), "different semantic result");
        assert.equal(values[0].metrics.presentedFrames, values[1].metrics.presentedFrames, "different frame work");
        pairs.push(values);
        // Persist immediately; retain every fixed pair and all failed evidence.
        await writeFile(path.join(output, `${workload}-${mode}.json`),
          stringifyEvidence({ sourceSha, workerLifetime: "fresh-per-pair-warmed",
            scoredWorkerWarmups: protocol.scoredWorkerWarmups, pairs }) + "\n");
      }
      const costs = Object.fromEntries(["creation_ms", "local_ms", "execution_ms"].map(key => [key,
        pairedCost(pairs.map(pair => pair[0][key]), pairs.map(pair => pair[1][key]))]));
      rows.push({ workload, mode, sourceSha, pairs, costs });
      for (const [key, cost] of Object.entries(costs)) {
        console.log(`${workload}/${mode}/${key}: ${cost.ratio.toFixed(4)}x [${cost.lower.toFixed(4)}, ${cost.upper.toFixed(4)}] ${cost.status}`);
        if (cost.status !== "pass") failures.push({ workload, mode, key, ...cost });
      }
    }
  }
  // Do not turn the old, loose 20% product threshold into a no-regression claim.
  for (const cohort of ["", "camera/"]) {
    const comparison = JSON.parse(await readFile(path.join(output, "..", cohort, "candidate/comparison.json"), "utf8"));
    const { fps, render } = qualifyProductMetrics(comparison);
    if (fps.status !== "pass") {
      failures.push({ kind: fps.status === "regression" ? "product_fps" : "product_fps_inconclusive",
        cohort, ...fps });
    }
    if (render !== null && render.status !== "pass") {
      failures.push({ kind: "product_render_cost", cohort, ...render });
    }
    for (const [name, value] of Object.entries(comparison.latency)) {
      if (value.candidateMs > value.baselineMs * 1.03 + 20) failures.push({ kind: "product_latency", cohort, name, ...value });
    }
  }
  // Profiles are a separate diagnostic experiment AFTER every prescribed pair.
  // Their instrumented durations never enter rows/costs or acceptance ratios.
  for (const workload of protocol.workloads) {
    for (const side of [0, 1]) {
      const count = profiles[side].length;
      const source = profileSource(workload);
      const observation = await measure(side, source, "async", workload);
      assert.equal(profiles[side].length, count + 1, "missing diagnostic profile");
      const profile = validateProfile(profiles[side].at(-1), workload);
      diagnostics.push({ side, sourceSha: createHash("sha256").update(source).digest("hex"),
        profile, observation });
      await writeFile(path.join(output, "profiles.json"), stringifyEvidence(diagnostics) + "\n");
    }
  }
  // The retained whole-scene profiles cover async only. Diagnose local edits
  // in all three actual source modes without profiling across their barriers.
  // All original trials/qualifications above have already completed; do not
  // feed these instrumented observations into timing rows or acceptance.
  for (const mode of protocol.modes) {
    const source = localEditProfileSource(mode);
    const sourceSha = createHash("sha256").update(source).digest("hex");
    await writeFile(path.join(output, `local-edit-profile-${mode}.py`), source);
    for (const side of [0, 1]) {
      const count = localProfiles[side].length;
      const observation = await measure(side, source, mode, "deterministic");
      assert.equal(localProfiles[side].length, count + 1, "missing local-edit profile");
      const profile = validateLocalEditProfile(localProfiles[side].at(-1), mode);
      localDiagnostics.push({ side, sourceSha, profile, observation });
      await writeFile(path.join(output, "local-edit-profiles.json"), stringifyEvidence(localDiagnostics) + "\n");
    }
  }
  // Same-package history control AFTER every acceptance measurement and profile.
  // The original worker retains the preceding runs; its fresh peer loads the
  // exact same verified package/server. No forced GC, profiling or threshold
  // adjustment. A timing difference here cannot erase an earlier failed row.
  for (const side of [0, 1]) {
    await diagnoseWorkerHistory({ identity: identities[side],
      retained: { page: pages[side], reports: reports[side] },
      openFresh: async () => {
        const context = await browser.newContext({ viewport: { width: 320, height: 180 } });
        try {
          await cache.install(context);
          const page = await context.newPage();
          const observations = [];
          page.on("console", message => {
            if (message.text().startsWith("NOON_PERF_REPORT ")) {
              observations.push(JSON.parse(message.text().slice(17)));
            }
          });
          page.on("pageerror", error => failures.push({ kind: "history_control_pageerror", side, message: String(error) }));
          await page.goto(`${servers[side].baseUrl}/web/execution-worker-smoke.html`);
          await page.evaluate(async () => {
            const { PythonAuthoringClient } = await import("./authoring-client.js");
            window.perfAuthoring = new PythonAuthoringClient();
            await window.perfAuthoring.ready();
          });
          return { page, reports: observations, close: () => context.close() };
        } catch (error) {
          await context.close();
          throw error;
        }
      },
      measure: (participant, source) => measure(side, source, "jspi", "deterministic", participant),
      record: evidence => writeFile(path.join(output, `worker-history-${side}.json`), stringifyEvidence(evidence) + "\n"),
    });
  }

  // Fresh-peer causal controls. Unlike the old retained/fresh diagnostic, both
  // workers begin from identical package initialization. Only treatment receives
  // the declared history. These observations are diagnostic-only and occur after
  // every acceptance measurement/profile; they cannot rescore an earlier row.
  for (const side of [0, 1]) {
    const openControlled = async name => {
      const context = await browser.newContext({ viewport: { width: 320, height: 180 } });
      try {
        await cache.install(context);
        const page = await context.newPage();
        const observations = [];
        page.on("console", message => {
          if (message.text().startsWith("NOON_PERF_REPORT ")) {
            observations.push(JSON.parse(message.text().slice(17)));
          }
        });
        page.on("pageerror", error => failures.push({
          kind: "controlled_history_pageerror", side, name, message: String(error),
        }));
        await page.goto(`${servers[side].baseUrl}/web/execution-worker-smoke.html`);
        await page.evaluate(async () => {
          const { PythonAuthoringClient } = await import("./authoring-client.js");
          window.perfAuthoring = new PythonAuthoringClient();
          await window.perfAuthoring.ready();
        });
        return { page, reports: observations, close: () => context.close() };
      } catch (error) {
        await context.close();
        throw error;
      }
    };
    const controlledMeasure = (participant, source, mode, workload) =>
      measure(side, source, mode, workload, participant);
    const repeated = Array.from({ length: 9 }, () => ({
      mode: "jspi", workload: "deterministic",
      source: performanceSource("jspi", "deterministic"),
    }));
    const heterogeneous = protocol.modes.flatMap(mode => protocol.workloads.map(workload => ({
      mode, workload, source: performanceSource(mode, workload),
    })));
    for (const [label, history] of [["repeated_same_source", repeated],
      ["heterogeneous_sources", heterogeneous]]) {
      await diagnoseControlledHistory({
        identity: identities[side], openFresh: openControlled,
        measure: controlledMeasure, history, label,
        record: evidence => writeFile(
          path.join(output, `controlled-history-${side}-${label}.json`),
          stringifyEvidence(evidence) + "\n"),
      });
    }
  }
} catch (error) {
  failures.push({ kind: "execution", message: String(error), stack: error.stack });
} finally {
  // Preserve the actual pinned interpreter bytes for offline reproduction.
  // Hash-addressed filenames cannot turn resource URLs into arbitrary paths.
  try {
    const directory = path.join(output, "interpreter-inputs");
    await mkdir(directory, { recursive: true });
    const assets = [];
    for (const { url, status, headers, body } of await cache.snapshot()) {
      const sha256 = createHash("sha256").update(body).digest("hex");
      const file = `${sha256}.bin`;
      await writeFile(path.join(directory, file), body);
      assets.push({ url, status, headers, file, sha256, bytes: body.length });
    }
    await writeFile(path.join(directory, "manifest.json"), JSON.stringify({ schema: 1, assets }, null, 2) + "\n");
  } catch (error) {
    failures.push({ kind: "diagnostic_inputs", message: String(error) });
  }
  await writeFile(path.join(output, "comparison.json"), stringifyEvidence({ schema: 1, protocol, identities, changedBuildInputs,
    host: { cpu: os.cpus()[0]?.model, platform: os.platform(), arch: os.arch(), node: process.version },
    warmups, rows, failures, cache: cache.stats() }) + "\n");
  for (const context of contexts) await context.close();
  await browser?.close();
  for (const server of servers) await server.close();
}
assert.deepEqual(failures, [], "performance regression or inconclusive qualification; all samples retained");
