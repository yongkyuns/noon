// #1933: manual physical-GPU prerequisite check, NOT product performance qualification.
// The existing frozen 7+3 product cohorts and host-cost acceptance are unaffected.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { writeFile, mkdir } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const EXPECTED = Object.freeze({
  platform: "darwin", node: "v22.23.3", chromium: "151.0.7922.34",
  backend: "WebGL2", mode: "hardware", minimumCores: 4,
});

const software = /swiftshader|llvmpipe|softpipe|software (?:renderer|rasterizer|adapter)|microsoft basic render/i;

export function powerSource(pmsetOutput) {
  if (/Now drawing from ['\"]AC Power['\"]/.test(pmsetOutput)) return "ac";
  if (/Now drawing from ['\"]Battery Power['\"]/.test(pmsetOutput)) return "battery";
  return "unknown";
}

export function classifyWebgl(readback) {
  if (!readback || readback.backend !== "WebGL2" || readback.error) return "unknown";
  const vendor = String(readback.unmaskedVendor ?? "").trim();
  const renderer = String(readback.unmaskedRenderer ?? "").trim();
  if (!vendor || !renderer) return "unknown";
  if (software.test(vendor + " " + renderer)) return "software";
  return "hardware-like-unverified"; // Cannot certify physical GPU provenance by string alone.
}

export function assessReadiness(observed, expected = EXPECTED) {
  const reasons = [];
  if (observed.platform !== expected.platform) reasons.push("not a physical macOS runtime");
  if (observed.node !== expected.node) reasons.push("Node release differs from pinned version");
  if (observed.browser !== expected.chromium) reasons.push("Chromium differs from pinned product browser");
  if (!(Number.isInteger(observed.cores) && observed.cores >= expected.minimumCores)) {
    reasons.push("insufficient identified logical cores");
  }
  if (observed.power !== "ac") reasons.push("AC power not independently established");
  const classification = classifyWebgl(observed.webgl);
  if (classification !== "hardware-like-unverified") reasons.push("actual WebGL2 hardware backing unverified");
  if (observed.webgl?.readbackValid !== true) reasons.push("WebGL2 clear/readback proof failed");
  if (observed.webgl?.contextLost === true) reasons.push("WebGL2 context was lost");
  return {
    status: reasons.length ? "blocked" : "eligible-for-separate-aa-diagnostic",
    blockers: reasons,
    rendererClassification: classification,
    qualification: false,
    mergeApproval: false,
    performanceAcceptance: false,
  };
}

export function browserProbeSource() {
  // Keep this short and standalone: no fake engine canvas, no interpreted FPS score.
  return async () => {
    const canvas = document.createElement("canvas");
    canvas.width = 64; canvas.height = 64;
    const gl = canvas.getContext("webgl2", {
      antialias: false, preserveDrawingBuffer: true, powerPreference: "high-performance",
    });
    if (!gl) return { backend: "missing", error: "WebGL2 context unavailable" };
    const debug = gl.getExtension("WEBGL_debug_renderer_info");
    const unmaskedVendor = debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : "";
    const unmaskedRenderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : "";
    gl.clearColor(0.25, 0.5, 0.75, 1.0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    const pixel = new Uint8Array(4);
    gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
    const readbackValid = [64, 128, 191, 255].every((v, i) => Math.abs(pixel[i] - v) <= 2);
    const err = gl.getError();
    return {
      backend: "WebGL2", vendor: gl.getParameter(gl.VENDOR),
      renderer: gl.getParameter(gl.RENDERER),
      unmaskedVendor, unmaskedRenderer,
      version: gl.getParameter(gl.VERSION), pixel: [...pixel],
      readbackValid: readbackValid && err === gl.NO_ERROR,
      glError: err,
      contextLost: gl.isContextLost(),
    };
  };
}

function safeCommand(cmd, args = []) {
  try {
    return execFileSync(cmd, args, { encoding: "utf8", timeout: 6000, maxBuffer: 64 * 1024,
      stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return "";
  }
}

function snapshot() {
  return {
    platform: process.platform, node: process.version,
    cores: os.availableParallelism(), cpuModel: os.cpus()[0]?.model ?? null,
    model: safeCommand("sysctl", ["-n", "hw.model"]) || null,
    power: powerSource(safeCommand("pmset", ["-g", "batt"])),
    loadAverage: os.loadavg(), memoryBytes: os.totalmem(),
  };
}

export async function runReadiness({ launch, flags, outfile }) {
  assert.ok(outfile && typeof outfile === "string", "required evidence file");
  const observation = {
    schema: 1, purpose: "physical-runner-preflight-only",
    testedCommit: safeCommand("git", ["rev-parse", "HEAD"]) || null,
    generatedAt: new Date().toISOString(),
    workflowRunId: process.env.GITHUB_RUN_ID ?? null,
    workflowAttempt: process.env.GITHUB_RUN_ATTEMPT ?? null,
    configuration: { backend: "webgl", gpuMode: "hardware", browserHeadless: false,
      pinnedBrowser: EXPECTED.chromium },
    observed: { ...snapshot(), browser: null, webgl: null },
    errors: [],
  };
  let browser;
  try {
    assert.deepEqual(flags, [
      "--disable-features=WebGPU", "--use-gpu-in-tests", "--ignore-gpu-blocklist",
      "--force-high-performance-gpu", "--disable-gpu-sandbox", "--disable-dev-shm-usage",
    ], "hardware flags must be exactly the shared renderer support flags");
    browser = await launch({ headless: false, args: flags, timeout: 30_000 });
    observation.observed.browser = browser.version();
    const context = await browser.newContext({ viewport: { width: 64, height: 64 } });
    try {
      const page = await context.newPage();
      await page.goto("about:blank");
      observation.observed.webgl = await page.evaluate(browserProbeSource());
    } finally { await context.close(); }
  } catch (error) {
    observation.errors.push(String(error?.stack ?? error));
  } finally {
    if (browser) await browser.close().catch(error => observation.errors.push(String(error)));
  }
  observation.decision = assessReadiness(observation.observed);
  if (observation.errors.length) {
    observation.decision.status = "blocked";
    observation.decision.blockers.push("browser readiness probe failed");
  }
  // Exclusive write; never overwrite an earlier observation, even a blocked one.
  await mkdir(path.dirname(outfile), { recursive: true });
  await writeFile(outfile, JSON.stringify(observation, null, 2) + "\n", { flag: "wx" });
  return observation;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const outfile = process.argv[2];
  assert.ok(outfile && process.argv.length === 3, "usage: node perf-physical-readiness.mjs OUTPUT.json");
  // Dynamic imports keep the pure admission tests independent of Playwright and
  // make the existing GPU mode/flags implementation authoritative.
  const [{ default: playwright }, { browserArgs }] = await Promise.all([
    import("playwright"), import("./manim-raster-support.mjs"),
  ]);
  const result = await runReadiness({ launch: options => playwright.chromium.launch(options),
    flags: browserArgs("webgl", { gpuMode: "hardware" }), outfile });
  console.log(JSON.stringify({ status: result.decision.status, blockers: result.decision.blockers,
    gpu: result.observed.webgl?.unmaskedRenderer ?? null }));
  if (result.decision.status !== "eligible-for-separate-aa-diagnostic") process.exitCode = 1;
}
