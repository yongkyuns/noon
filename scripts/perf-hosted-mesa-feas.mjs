// #1933: preregistered hosted-only browser backend *feasibility*, never FPS acceptance.
// No scene source, threshold, scorer, original performance artifact, or runtime source changed.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { browserArgs } from "./manim-raster-support.mjs";

export const STUDY = "1933-hosted-mesa-feas-20261009-01";
export const EXPECTED_BROWSER = "151.0.7922.34";
export const PINNED_MODES = Object.freeze([
  { id: "frozen-swiftshader-control", angle: "swiftshader", mesa: false },
  { id: "mesa-angle-gl", angle: "gl", mesa: true },
  { id: "mesa-angle-gl-egl", angle: "gl-egl", mesa: true },
]);

function mesaArgs(angle) {
  assert.ok(["gl", "gl-egl"].includes(angle), "unregistered ANGLE choice");
  return [
    "--disable-features=WebGPU",
    "--use-gpu-in-tests",
    "--ignore-gpu-blocklist",
    "--use-gl=angle",
    "--use-angle=" + angle,
    "--disable-gpu-sandbox",
    "--disable-dev-shm-usage",
  ];
}

export function launchForMode(mode) {
  assert.ok(PINNED_MODES.some(x => x.id === mode.id && x.angle === mode.angle && x.mesa === mode.mesa),
    "not a preregistered mode");
  const env = { ...process.env };
  if (mode.mesa) {
    env.LIBGL_ALWAYS_SOFTWARE = "true";
    env.GALLIUM_DRIVER = "llvmpipe";
    env.LP_NUM_THREADS = "2";
    if (mode.angle === "gl-egl") env.EGL_PLATFORM = "surfaceless";
  }
  const args = mode.mesa ? mesaArgs(mode.angle) : browserArgs("webgl");
  return { headless: false, args, env, timeout: 30000 };
}

function approximate(actual, expected, tol = 3) {
  return Array.isArray(actual) && actual.length === 4 &&
    expected.every((n, i) => Math.abs(actual[i] - n) <= tol);
}

export function classifyMode(mode, observed) {
  const renderer = String(observed?.unmaskedRenderer || "");
  const matched = mode.mesa ? /llvmpipe/i.test(renderer) && !/swiftshader/i.test(renderer)
    : /swiftshader/i.test(renderer);
  const errors = [];
  if (observed?.browser !== EXPECTED_BROWSER) errors.push("wrong Chromium revision");
  if (observed?.backend !== "WebGL2") errors.push("missing WebGL2");
  if (!renderer) errors.push("unmasked driver identity unavailable");
  else if (!matched) errors.push("expected rendering backend not observed");
  if (!approximate(observed?.clearPixel, [51, 102, 153, 255])) errors.push("clear/readPixels invalid");
  if (!approximate(observed?.trianglePixel, [153, 51, 102, 255]))
    errors.push("real triangle draw/readPixels invalid");
  if (observed?.linkSuccess !== true) errors.push("shader program not linked");
  if (observed?.contextLost !== false || observed?.glError !== 0) errors.push("WebGL2 context or GL error");
  return { id: mode.id, status: errors.length ? "not-confirmed" : "usable-software-webgl2-backend",
    blockers: errors, renderer, noHardwareClaim: true,
    qualification: false, performanceAcceptance: false, mergeApproval: false };
}

export async function browserProbe() {
  const canvas = document.createElement("canvas");
  canvas.width = 64; canvas.height = 64;
  const gl = canvas.getContext("webgl2", { preserveDrawingBuffer: true, antialias: false });
  if (!gl) return { backend: "missing", error: "WebGL2 unavailable" };
  const debug = gl.getExtension("WEBGL_debug_renderer_info");
  const unmaskedRenderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : "";
  const unmaskedVendor = debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : "";
  gl.viewport(0, 0, 64, 64);
  gl.clearColor(0.2, 0.4, 0.6, 1.0);
  gl.clear(gl.COLOR_BUFFER_BIT);
  const px = new Uint8Array(4);
  gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
  const clearPixel = Array.from(px);

  function shader(type, source) {
    const s = gl.createShader(type);
    gl.shaderSource(s, source);
    gl.compileShader(s);
    return { source: s, valid: Boolean(gl.getShaderParameter(s, gl.COMPILE_STATUS)),
      log: gl.getShaderInfoLog(s) || "" };
  }
  const vs = shader(gl.VERTEX_SHADER, "#version 300 es\nvoid main() {\n vec2 pos[3] = vec2[3](vec2(-1.,-1.), vec2(3.,-1.), vec2(-1.,3.));\n gl_Position = vec4(pos[gl_VertexID], 0., 1.);\n}");
  const fs = shader(gl.FRAGMENT_SHADER, "#version 300 es\nprecision highp float;\nout vec4 outColor;\nvoid main() { outColor=vec4(0.6,0.2,0.4,1.0); }");
  let linkSuccess = false, trianglePixel = null, linkLog = "";
  if (vs.valid && fs.valid) {
    const p = gl.createProgram();
    gl.attachShader(p, vs.source); gl.attachShader(p, fs.source);
    gl.linkProgram(p);
    linkSuccess = Boolean(gl.getProgramParameter(p, gl.LINK_STATUS));
    linkLog = gl.getProgramInfoLog(p) || "";
    if (linkSuccess) {
      gl.useProgram(p);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
      trianglePixel = Array.from(px);
    }
  }
  return {
    backend: "WebGL2", unmaskedRenderer, unmaskedVendor,
    renderer: gl.getParameter(gl.RENDERER), vendor: gl.getParameter(gl.VENDOR),
    version: gl.getParameter(gl.VERSION),
    clearPixel, trianglePixel, linkSuccess, shaderValid: vs.valid && fs.valid,
    shaderLog: [vs.log, fs.log, linkLog].filter(Boolean).join("\n"),
    glError: gl.getError(), contextLost: gl.isContextLost(),
  };
}

export async function acquire({ launch, outfile, plan }) {
  assert.equal(plan.studyId, STUDY, "wrong preregistered study");
  assert.deepEqual(plan.scenarios.map(x => x.id), PINNED_MODES.map(x => x.id),
    "fixed scenario order changed");
  const record = {
    schema: 1, studyId: STUDY, diagnosticOnly: true, qualification: false,
    performanceAcceptance: false, mergeApproval: false,
    workflowAttempt: process.env.GITHUB_RUN_ATTEMPT ?? null,
    node: process.version, platform: process.platform,
    availableCores: os.availableParallelism(), cpuModel: os.cpus()[0]?.model ?? null,
    display: Boolean(process.env.DISPLAY), cases: [],
  };
  for (const mode of PINNED_MODES) {
    const opts = launchForMode(mode);
    const evidence = { id: mode.id, flags: opts.args, env: Object.fromEntries(
      ["LIBGL_ALWAYS_SOFTWARE", "GALLIUM_DRIVER", "LP_NUM_THREADS", "EGL_PLATFORM"]
        .map(k => [k, opts.env[k] ?? null])), observation: null, errors: [] };
    let browser = null;
    try {
      browser = await launch(opts);
      const context = await browser.newContext({ viewport: { width: 64, height: 64 } });
      try {
        const page = await context.newPage();
        await page.goto("about:blank");
        evidence.observation = { browser: browser.version(), ...await page.evaluate(browserProbe) };
      } finally { await context.close(); }
    } catch (e) {
      evidence.errors.push(String(e?.stack ?? e));
    } finally {
      if (browser) await browser.close().catch(e => evidence.errors.push(String(e)));
    }
    evidence.decision = classifyMode(mode, evidence.observation);
    if (evidence.errors.length) {
      evidence.decision.status = "not-confirmed";
      evidence.decision.blockers.push("browser launch or acquisition error");
    }
    record.cases.push(evidence);
  }
  record.acquisitionComplete = record.cases.length === PINNED_MODES.length;
  record.anyMesaEligible = record.cases.filter(c => PINNED_MODES.find(m => m.id === c.id)?.mesa)
    .some(c => c.decision.status === "usable-software-webgl2-backend");
  // Evidence is created once and is never overwritten or favorably re-scored.
  await mkdir(path.dirname(outfile), { recursive: true });
  await writeFile(outfile, JSON.stringify(record, null, 2) + "\n", { flag: "wx" });
  return record;
}
async function main() {
  const [planPath, outfile] = process.argv.slice(2);
  assert.ok(planPath && outfile && process.argv.length === 4,
    "usage: node perf-hosted-mesa-feas.mjs PLAN.json OUT.json");
  const plan = JSON.parse(await readFile(planPath, "utf8"));
  const { default: playwright } = await import("playwright");
  const record = await acquire({ launch: opts => playwright.chromium.launch(opts), outfile, plan });
  console.log(JSON.stringify({ studyId: STUDY, complete: record.acquisitionComplete,
    anyMesaEligible: record.anyMesaEligible,
    cases: record.cases.map(c => ({ id: c.id, status: c.decision.status,
      renderer: c.decision.renderer, blockers: c.decision.blockers })) }));
  if (!record.acquisitionComplete) process.exitCode = 1;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(e => { console.error(e); process.exitCode = 1; });
}
