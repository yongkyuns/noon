// Temporary #2056 diagnostic. Ablations never become public gallery sources.
// Same exact ray timing, solver and captures; only selected rendering features
// are disabled. Uses the EXISTING source/engine/renderer profiling host.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const sha256 = value => createHash("sha256").update(value).digest("hex");

export function diagnosticSource(source, { glow = true, laboratory = true } = {}) {
  assert.equal((source.match(/class TriangleLightShowcase\(Scene\):/g) ?? []).length, 1);
  let result = source;
  if (!glow) {
    // Each effect call is a complete simple statement in this authored scene.
    // Keep all three ray shapes and the photon, and every play/wait unchanged.
    let count = 0;
    result = result.replace(/^(\s*)(?:photon|halo|arc)\.set_glow\([^\n]+\)\s*$/gm,
      (_, indent) => { count++; return `${indent}pass  # diagnostic: disable only Gaussian glow`; });
    assert.equal(count, 4, "source glow call sites changed; review the ablation");
  }
  if (!laboratory) {
    const anchor = "        room, arms, mouth, clock_hands = laboratory_artwork()";
    assert.equal(result.split(anchor).length, 2, "laboratory admission changed");
    // Keep construction cost identical, but exclude scenery from the DRAWN
    // group. Include the pose targets so the complete scene remains executable.
    result = result.replace(anchor, `${anchor}\n        room = [Rectangle(width=16, height=9, fill=ink("#734934"),\n                          fill_opacity=1, stroke=None), *arms, *clock_hands]`);
  }
  return result;
}

export function distribution(values) {
  assert.ok(values.length > 0, "a stage with no samples is unavailable, not zero");
  assert.ok(values.every(x => typeof x === "number" && Number.isFinite(x) && x >= 0));
  const sorted = [...values].sort((a,b) => a-b);
  const at = q => sorted[Math.max(0, Math.ceil(q * sorted.length)-1)];
  return { samples:values.length, median:at(0.5), p95:at(0.95), max:at(1),
    mean:values.reduce((a,b)=>a+b,0)/values.length };
}

export function summarizeProfile(report) {
  assert.equal(report?.schemaVersion, 2, "profile schema changed");
  assert.equal(report.environment?.rendererBackend, "WebGL2");
  assert.equal(report.execution?.sourceContinuation, true);
  assert.equal(report.execution?.requestedMeasuredFrames, 90);
  assert.ok(Math.abs(report.execution?.firstMeasuredTime - 25/30) < 1e-8);
  assert.ok(Math.abs(report.execution?.lastMeasuredTime - 114/30) < 1e-8);
  assert.equal(report.samples?.length, 90);
  const stages = {};
  for (const [field, keys] of [
    ["stageTimingSamples", ["rustDriveMs", "deltaDrainMs", "deltaMetadataMs", "deltaSendMs",
      "presentationWaitMs", "segmentHandoffMs", "authoringBoundaryWaitMs"]],
    ["rendererSubstageSamples", ["surfaceAcquireCpuWallMs", "prepareCpuWallMs", "uploadCpuWallMs",
      "encodeCpuWallMs", "submitPresentCpuWallMs"]],
    ["rendererSamples", ["lastDeltaApplyMs", "lastRendererCallMs", "uploadBytes", "drawCalls"]],
  ]) {
    assert.ok(Array.isArray(report[field]) && report[field].length > 0, `${field} unavailable`);
    for (const key of keys) stages[key] = distribution(report[field].map(row => row[key]));
  }
  return { setup:report.setup, pipeline:report.pipeline, cadence:report.cadence,
    environment:report.environment, renderer:report.renderer, stages };
}

async function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const runtimeRoot = path.resolve(process.env.NOON_TRIANGLE_PROFILE_RUNTIME ?? root);
  const output = path.resolve(process.env.NOON_TRIANGLE_PROFILE_OUTPUT ?? path.join(root,"browser-smoke-artifacts/triangle-profile"));
  const source = await readFile(path.join(root,"web/python/examples/showcase_triangle_light.py"),"utf8");
  const runtime = JSON.parse(await readFile(path.join(runtimeRoot,"web/runtime-build-identity.json"),"utf8"));
  const expected = process.env.NOON_TRIANGLE_PROFILE_ENGINE_SHA;
  if (expected) assert.equal(runtime.sourceRevision,expected,"wrong diagnostic release engine");
  const sourceRevision = execFileSync("git",["rev-parse","HEAD"],{cwd:root,encoding:"utf8"}).trim();
  await mkdir(output,{recursive:true});
  const { chromium } = await import("playwright");
  const { serveRepository } = await import("./browser-test-server.mjs");
  const { browserArgs } = await import("./manim-raster-support.mjs");
  const { createPyodideResourceCache } = await import("./pyodide-resource-cache.mjs");
  const server = await serveRepository(runtimeRoot,0);
  const launchArgs = browserArgs("webgl");
  const browser = await chromium.launch({headless:true,args:launchArgs});
  const cache = createPyodideResourceCache(await readFile(path.join(runtimeRoot,"web/python-worker.js"),"utf8"));
  const report = { purpose:"Controlled bottleneck isolation, not hardware-FPS or gallery qualification",
    sourceRevision, sourceSha256:sha256(source), runtime, launchArgs,
    browserVersion:browser.version(), sampleWindow:[25/30,114/30],
    note:"Release engine and authored source revisions are recorded separately; never relabeled as an exact-head build. No video recording. All cases use identical timing and all 10084 collisions/exposure resources.",
    cases:[] };
  try {
    for (const variant of [
      {id:"full",glow:true,laboratory:true},
      {id:"no-glow",glow:false,laboratory:true},
      {id:"no-laboratory",glow:true,laboratory:false},
      {id:"no-glow-no-laboratory",glow:false,laboratory:false},
      {id:"full-repeat",glow:true,laboratory:true},
    ]) {
      const code = diagnosticSource(source,variant);
      const result = {...variant, sourceSha256:sha256(code),outcome:"fail"};
      report.cases.push(result);
      await writeFile(path.join(output,variant.id+".py"),code);
      const context = await browser.newContext({viewport:{width:1200,height:900},deviceScaleFactor:1});
      await cache.install(context);
      const page = await context.newPage();
      const errors=[];
      page.on("pageerror",error=>errors.push(String(error)));
      try {
        await page.route("**/python/demo_scene.py",route=>route.fulfill({contentType:"text/plain",body:code}));
        const params = new URLSearchParams({ source:"./python/demo_scene.py",warmup:"24",frames:"90",targetHz:"30",
          includeSamples:"1",includeRendererSamples:"1",includeStageTimings:"1",includeRendererGpuIdentity:"1",
          rendererMetricsSampling:"dense" });
        const started = performance.now();
        await page.goto(`${server.baseUrl}/web/scene-perf.html?${params}`);
        await page.waitForFunction(()=>["complete","error"].includes(document.querySelector("#status")?.dataset.state),null,{timeout:240000});
        const observed = await page.evaluate(()=>({state:document.querySelector("#status").dataset.state,
          status:document.querySelector("#status").value,report:window.__NOON_SCENE_PERF__}));
        result.totalDiagnosticWallMs=performance.now()-started;
        result.pageErrors=errors;
        await writeFile(path.join(output,variant.id+"-raw.json"),JSON.stringify(observed,null,2));
        assert.equal(observed.state,"complete",observed.status);
        assert.deepEqual(errors,[]);
        assert.deepEqual(observed.report.runtimeBuild,runtime);
        result.summary=summarizeProfile(observed.report);
        result.outcome="pass";
        console.log(`PASS profile ${variant.id}: ${JSON.stringify(result.summary.stages)}`);
      } catch(error) {
        result.error=String(error.stack??error);
        result.pageErrors=errors;
        console.error(`FAIL profile ${variant.id}: ${result.error}`);
      } finally {
        await context.close();
        await writeFile(path.join(output,"report.json"),JSON.stringify(report,null,2));
      }
    }
  } finally { await browser.close(); await server.close(); }
  assert.ok(report.cases.every(row=>row.outcome==="pass"),"one or more diagnostic variants failed; inspect retained evidence");
}
if (process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) await main();
