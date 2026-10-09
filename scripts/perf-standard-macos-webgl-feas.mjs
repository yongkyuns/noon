// #1933: Standard GitHub-hosted macOS GPU feasibility only, not qualification.
// Fixed one-shot headed/headless Chromium probes; no scored product/host timings.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { browserArgs } from "./manim-raster-support.mjs";
import { classifyWebgl } from "./perf-physical-readiness.mjs";

export const STUDY_ID = "1933-standard-macos-webgl-feas-20261009-01";
export const PINNED_CASES = Object.freeze([
  { id: "headed", headless: false },
  { id: "headless", headless: true },
]);

export function assess(caseId, observed) {
  assert.ok(PINNED_CASES.some(c => c.id === caseId), "unregistered case");
  const blockers = [];
  if (observed?.browser !== "151.0.7922.34") blockers.push("wrong Chromium revision");
  if (observed?.backend !== "WebGL2") blockers.push("WebGL2 context unavailable");
  const classification = classifyWebgl(observed);
  if (classification !== "hardware-like-unverified") {
    blockers.push("actual nonsoftware browser renderer not established");
  }
  for (const [name, expected] of [
    ["clearPixel", [51, 102, 153, 255]],
    ["trianglePixel", [153, 51, 102, 255]],
  ]) {
    if (!Array.isArray(observed?.[name]) || observed[name].length !== expected.length
      || !expected.every((n, i) => Math.abs(observed[name][i] - n) <= 3)) {
      blockers.push(name + " invalid");
    }
  }
  if (observed?.shaderLinked !== true) blockers.push("shader compile/link did not succeed");
  if (observed?.glError !== 0 || observed?.contextLost !== false) {
    blockers.push("WebGL2 error or context loss");
  }
  return {
    caseId, status: blockers.length ? "not-confirmed" : "eligible-for-separate-aa-only",
    classification, blockers, qualification: false, performanceAcceptance: false,
    mergeApproval: false, physicalGpuProven: false,
  };
}

export function gpuProbeSource() {
  return () => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const gl = canvas.getContext("webgl2", {
      antialias: false, preserveDrawingBuffer: true, powerPreference: "high-performance",
    });
    if (!gl) return { backend: "missing", error: "WebGL2 unavailable" };
    const dbg = gl.getExtension("WEBGL_debug_renderer_info");
    const unmaskedVendor = dbg ? String(gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL)) : "";
    const unmaskedRenderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : "";
    const vendor = String(gl.getParameter(gl.VENDOR));
    const renderer = String(gl.getParameter(gl.RENDERER));
    gl.viewport(0, 0, 64, 64);
    gl.clearColor(0.2, 0.4, 0.6, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const pixel = new Uint8Array(4);
    gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    const clearPixel = [...pixel];
    function compile(kind, source) {
      const shader = gl.createShader(kind);
      gl.shaderSource(shader, source);
      gl.compileShader(shader);
      return { shader, ok: Boolean(gl.getShaderParameter(shader, gl.COMPILE_STATUS)),
        log: gl.getShaderInfoLog(shader) || "" };
    }
    const vs = compile(gl.VERTEX_SHADER, "#version 300 es\nvoid main(){vec2 p[3]=vec2[3](vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));gl_Position=vec4(p[gl_VertexID],0.,1.);}");
    const fs = compile(gl.FRAGMENT_SHADER, "#version 300 es\nprecision highp float;\nout vec4 color;\nvoid main(){color=vec4(0.6,0.2,0.4,1.);}");
    let shaderLinked = false, trianglePixel = null, programLog = null;
    if (vs.ok && fs.ok) {
      const program = gl.createProgram();
      gl.attachShader(program, vs.shader); gl.attachShader(program, fs.shader);
      gl.linkProgram(program);
      shaderLinked = Boolean(gl.getProgramParameter(program, gl.LINK_STATUS));
      programLog = gl.getProgramInfoLog(program) || "";
      if (shaderLinked) {
        gl.useProgram(program);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
        gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        trianglePixel = [...pixel];
      }
    }
    return { backend: "WebGL2", unmaskedVendor, unmaskedRenderer, vendor, renderer,
      version: String(gl.getParameter(gl.VERSION)),
      clearPixel, trianglePixel, shaderLinked,
      shaderLogs: [vs.log, fs.log, programLog].filter(Boolean),
      glError: gl.getError(), contextLost: gl.isContextLost() };
  };
}

function systemInfo(command, args = []) {
  try {
    return execFileSync(command, args, { encoding: "utf8", timeout: 12000,
      maxBuffer: 2 * 1024 * 1024, stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch (e) {
    return "unavailable: " + String(e?.message ?? e);
  }
}

export async function acquire({ plan, launch, outfile }) {
  assert.equal(plan.studyId, STUDY_ID);
  assert.equal(plan.runner, "macos-15");
  assert.equal(plan.node, "22.23.3");
  assert.equal(plan.playwright, "1.62.1");
  assert.equal(plan.chromium, "151.0.7922.34");
  assert.deepEqual(plan.cases, PINNED_CASES);
  assert.equal(plan.qualification, false);
  assert.equal(plan.performanceAcceptance, false);
  assert.equal(plan.mergeApproval, false);
  const evidence = {
    schema: 1, studyId: STUDY_ID, diagnosticOnly: true,
    qualification: false, performanceAcceptance: false, mergeApproval: false,
    environment: {
      platform: process.platform, arch: process.arch, node: process.version,
      cpuCount: os.availableParallelism(), cpu: os.cpus()[0]?.model,
      runnerOS: process.env.RUNNER_OS || null,
      runnerArch: process.env.RUNNER_ARCH || null,
      imageOS: process.env.ImageOS || null, imageVersion: process.env.ImageVersion || null,
      workflowRun: process.env.GITHUB_RUN_ID || null,
      attempt: process.env.GITHUB_RUN_ATTEMPT || null,
      systemProfilerDisplays: systemInfo("system_profiler", ["SPDisplaysDataType", "-json"]),
      pmset: systemInfo("pmset", ["-g", "batt"]),
    },
    cases: [],
  };
  if (process.platform !== "darwin" || process.arch !== "arm64") {
    throw new Error("standard macOS arm64 host was not provided");
  }
  if (process.version !== "v22.23.3") throw new Error("Node version differs");
  const flags = browserArgs("webgl", { gpuMode: "hardware" });
  assert.ok(flags.includes("--force-high-performance-gpu"), "pinned shared browser hardware flags unavailable");
  assert.ok(!flags.some(f => /swiftshader|llvmpipe/i.test(f)),
    "probe must not override Chromium into software rendering");
  for (const testCase of PINNED_CASES) {
    const record = { ...testCase, flags, observation: null, errors: [], decision: null };
    let browser;
    try {
      browser = await launch({ headless: testCase.headless, args: flags, timeout: 30000 });
      const context = await browser.newContext({ viewport: { width: 64, height: 64 } });
      try {
        const page = await context.newPage();
        await page.goto("about:blank");
        record.observation = { browser: browser.version(), ...await page.evaluate(gpuProbeSource()) };
      } finally { await context.close(); }
    } catch (e) {
      record.errors.push(String(e?.stack ?? e));
    } finally {
      if (browser) await browser.close().catch(e => record.errors.push(String(e)));
    }
    record.decision = assess(testCase.id, record.observation);
    if (record.errors.length) {
      record.decision.status = "not-confirmed";
      record.decision.blockers.push("browser startup or acquisition error");
    }
    evidence.cases.push(record);
  }
  evidence.complete = evidence.cases.length === PINNED_CASES.length;
  evidence.anyEligible = evidence.cases.some(c => c.decision.status === "eligible-for-separate-aa-only");
  await mkdir(path.dirname(outfile), { recursive: true });
  await writeFile(outfile, JSON.stringify(evidence, null, 2) + "\n", { flag: "wx" });
  return evidence;
}

async function main() {
  assert.equal(process.argv.length, 4, "usage: node perf-standard-macos-webgl-feas.mjs PLAN.json OUTPUT.json");
  const plan = JSON.parse(await readFile(process.argv[2], "utf8"));
  const { default: playwright } = await import("playwright");
  const result = await acquire({ plan, launch: opts => playwright.chromium.launch(opts),
    outfile: process.argv[3] });
  console.log(JSON.stringify({ studyId: STUDY_ID, complete: result.complete,
    anyEligible: result.anyEligible, cases: result.cases.map(c => ({
      id: c.id, renderer: c.observation?.unmaskedRenderer ?? null,
      vendor: c.observation?.unmaskedVendor ?? null,
      status: c.decision.status, blockers: c.decision.blockers,
    })) }));
  if (!result.complete) process.exitCode = 1;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(e => { console.error(e); process.exitCode = 1; });
}
