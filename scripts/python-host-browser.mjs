// Unchanged Python files, real Pyodide + WASM engine + actual browser renderer.
// Native observations are produced by python_host_conformance.py, never mocked.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { assertRenderedOutput, assertSingleReport, stringifyEvidence } from "./python-host-report.mjs";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";
import { disableAuthoringJspi } from "./playground-browser-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const evidence = path.resolve(process.env.NOON_PYTHON_HOST_EVIDENCE ?? path.join(root, "artifacts/python-host"));
const native = JSON.parse(await readFile(path.join(evidence, "native.json"), "utf8"));
assert.equal(native.schema, 1);
assert.equal(native.host, "native-cpython");
assert.equal(native.sample_hz, 4);
assert.equal(native.cases.length, 8);
const cache = createPyodideResourceCache(await readFile(path.join(root, "web/python-worker.source.js"), "utf8"));
const server = await serveRepository(root, 0, { crossOriginIsolated: true });
const results = [];

function compare(actual, expected, where = "report") {
  if (typeof expected === "number") {
    assert.equal(typeof actual, "number", where);
    assert.ok(Number.isFinite(actual) && Math.abs(actual - expected) <= 2e-5,
      `${where}: ${actual} != ${expected}`);
  } else if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), where);
    assert.equal(actual.length, expected.length, where);
    expected.forEach((v, i) => compare(actual[i], v, `${where}[${i}]`));
  } else if (expected !== null && typeof expected === "object") {
    assert.deepEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), where);
    for (const key of Object.keys(expected)) compare(actual[key], expected[key], `${where}.${key}`);
  } else assert.equal(actual, expected, where);
}

async function runCase(browser, backend, noJspi, expected) {
  const context = await browser.newContext({ viewport: { width: 640, height: 360 } });
  await cache.install(context);
  if (noJspi) await disableAuthoringJspi(context);
  const page = await context.newPage();
  const output = [], pageErrors = [];
  page.on("console", message => {
    const line = message.text();
    if (line.startsWith("NOON_HOST_REPORT ")) output.push(JSON.parse(line.slice(17)));
  });
  page.on("pageerror", error => pageErrors.push(String(error)));
  const source = await readFile(path.join(root, "parity/python-host", `${expected.case}.py`), "utf8");
  assert.equal(createHash("sha256").update(source).digest("hex"), expected.source_sha256);
  let timer;
  try {
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`);
    const result = await Promise.race([
      page.evaluate(async ({ source, expectFailure }) => {
        const { PythonAuthoringClient } = await import("./authoring-client.js");
        const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
        const { sampleOrSourceFailure } = await import("../scripts/python-host-sampling.mjs");
        const authoring = new PythonAuthoringClient();
        let settleRun, attachedResolve, attachedReject;
        const terminal = new Promise(resolve => { settleRun = resolve; });
        const attached = new Promise((resolve, reject) => { attachedResolve = resolve; attachedReject = reject; });
        attached.catch(() => {});
        const asynchronousErrors = [];
        const execution = new AuthoringExecutionClient(document.querySelector("#scene"), {
          onError(error) { asynchronousErrors.push(String(error)); attachedReject(error); },
        });
        let attachment;
        // Keep the rendered frame alive until Node captures it. Cleanup runs
        // after the observation, not when the Python source merely returns.
        window.noonHostCaseCleanup = async () => {
          await attachment?.catch(() => {});
          execution.terminate();
          authoring.terminate();
        };
        const started = performance.now();
        const authored = authoring.run(source, {}, {
          async onSemanticContinuation(registration) {
            attachment = execution.startSemanticExecution(registration.semanticExecution, {
              authoringClient: authoring, transportMode: "transferable", pacing: "external_samples",
            });
            try { attachedResolve(await attachment); }
            catch (error) { attachedReject(error); throw error; }
          },
        });
        authored.then(value => settleRun({ ok: true, value }), error => settleRun({ ok: false, message: String(error) }));
        const samples = [];
        let ready = null, metrics = null;
        try {
          const first = await Promise.race([
            attached.then(value => ({ attached: true, value })),
            terminal.then(value => ({ attached: false, value })),
          ]);
          if (first.attached) {
            ready = first.value;
            // Exact external input samples match the native finite 4 Hz profile.
            for (const time of [0, 0.25, 0.5, 0.75, 1]) {
              const begun = performance.now();
              const sample = await sampleOrSourceFailure(
                execution.sampleToAuthoredTime(time, { stopAtSourceCompletion: true }), terminal,
              );
              if (sample.terminal) break;
              samples.push({ requested: time, elapsedMs: performance.now() - begun, ...sample.sample });
              if (sample.sample.sourceCompleted) break;
            }
          }
          const finished = await terminal;
          if (finished.ok) metrics = (await execution.metrics()).metrics;
          return { terminal: finished, ready, samples, metrics, asynchronousErrors,
            elapsedMs: performance.now() - started, expectFailure };
        } catch (error) {
          await window.noonHostCaseCleanup();
          throw error;
        }
      }, { source, expectFailure: expected.terminal !== null }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${backend}/${expected.case} timed out`)), 120000); }),
    ]);
    // Persist the complete observation before assertions so a failing case is
    // reproducible too. Runtime u64 counters are BigInt in the JS binding.
    const caseName = `${backend}-${noJspi ? "no-jspi-" : ""}${expected.case}`;
    await writeFile(path.join(evidence, `${caseName}.json`),
      stringifyEvidence({ source_sha256: expected.source_sha256, output, pageErrors, result }) + "\n");
    assertSingleReport(output, expected.case, result);
    compare(output[0], expected.report, `${backend}/${expected.case}`);
    assert.equal(result.terminal.ok, expected.terminal === null, `${expected.case}: ${stringifyEvidence(result.terminal)}`);
    if (expected.terminal !== null) assert.match(result.terminal.message, new RegExp(expected.terminal));
    else {
      assert.deepEqual(result.asynchronousErrors, []);
      assertRenderedOutput(result.metrics, backend);
      assert.ok(Math.abs(result.metrics.time - result.terminal.value.duration) <= 2e-5,
        "final semantic completion was observed before its rendered endpoint");
    }
    assert.deepEqual(pageErrors, [], "unhandled page errors");
    const screenshot = `${backend}-${noJspi ? "no-jspi-" : ""}${expected.case}.png`;
    await page.locator("#scene").screenshot({ path: path.join(evidence, screenshot) });
    console.log(`PASS ${backend}/${noJspi ? "no-jspi/" : ""}${expected.case}: same source, same observations`);
    return { case: expected.case, backend, noJspi, source_sha256: expected.source_sha256,
      report: output[0], screenshot, ...result };
  } finally {
    clearTimeout(timer);
    try { await page.evaluate(() => window.noonHostCaseCleanup?.()); }
    finally { await context.close(); }
  }
}

await mkdir(evidence, { recursive: true });
try {
  for (const backend of ["webgpu", "webgl"]) {
    const browser = await playwright.chromium.launch({ headless: true, args: browserArgs(backend) });
    try {
      for (const expected of native.cases) results.push(await runCase(browser, backend, false, expected));
      // Prove fallback portability separately; do not pretend synchronous helpers
      // use JSPI when the source compiler actually inserted await statements.
      if (backend === "webgl") {
        for (const name of ["sequential", "callbacks", "portable"]) {
          results.push(await runCase(browser, backend, true, native.cases.find(c => c.case === name)));
        }
      }
    } finally { await browser.close(); }
  }
} finally {
  await writeFile(path.join(evidence, "browser.json"), stringifyEvidence({ schema: 1, results, cache: cache.stats() }) + "\n");
  await server.close();
}
assert.equal(results.length, 19);
