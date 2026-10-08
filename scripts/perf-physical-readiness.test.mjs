import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { EXPECTED, powerSource, classifyWebgl, assessReadiness, browserProbeSource,
  runReadiness } from "./perf-physical-readiness.mjs";

const flags = ["--disable-features=WebGPU", "--use-gpu-in-tests", "--ignore-gpu-blocklist",
  "--force-high-performance-gpu", "--disable-gpu-sandbox", "--disable-dev-shm-usage"];
const gl = { backend: "WebGL2", unmaskedVendor: "Apple Inc.",
  unmaskedRenderer: "ANGLE (Apple, Apple M4, Metal)", readbackValid: true,
  contextLost: false };
const ok = { platform: "darwin", node: "v22.23.3", browser: "151.0.7922.34",
  cores: 10, power: "ac", webgl: gl };

test("pin the original tested Chromium / Node / WebGL2 targets", () => {
  assert.equal(EXPECTED.chromium, "151.0.7922.34");
  assert.equal(EXPECTED.node, "v22.23.3");
  assert.equal(EXPECTED.backend, "WebGL2");
});
test("AC and battery detection fail closed", () => {
  assert.equal(powerSource("Now drawing from 'AC Power'"), "ac");
  assert.equal(powerSource("Now drawing from 'Battery Power'"), "battery");
  assert.equal(powerSource("No battery information available"), "unknown");
});
test("actual GPU requires identified renderer and rejects known software paths", () => {
  assert.equal(classifyWebgl(gl), "hardware-like-unverified");
  for (const name of ["Google SwiftShader", "llvmpipe", "software rasterizer", "Microsoft Basic Render Driver"]) {
    assert.equal(classifyWebgl({ ...gl, unmaskedRenderer: name }), "software");
  }
  assert.equal(classifyWebgl({ ...gl, unmaskedRenderer: "" }), "unknown");
  assert.equal(classifyWebgl({ ...gl, backend: "WebGL" }), "unknown");
  assert.equal(classifyWebgl(null), "unknown");
});
test("the positive classification is NOT a performance pass or GPU-provenance certificate", () => {
  const p = assessReadiness(ok);
  assert.equal(p.status, "eligible-for-separate-aa-diagnostic");
  assert.equal(p.rendererClassification, "hardware-like-unverified");
  assert.equal(p.qualification, false);
  assert.equal(p.mergeApproval, false);
  assert.equal(p.performanceAcceptance, false);
});
test("block failure and missing evidence independently", () => {
  const changes = [
    ["platform", "linux"], ["node", "v22.16.0"], ["browser", "unknown"],
    ["cores", 2], ["power", "battery"], ["power", "unknown"],
    ["webgl", { ...gl, unmaskedRenderer: "SwiftShader" }],
    ["webgl", { ...gl, readbackValid: false }],
    ["webgl", { ...gl, contextLost: true }], ["webgl", null],
  ];
  for (const [name, value] of changes) {
    const result = assessReadiness({ ...ok, [name]: value });
    assert.equal(result.status, "blocked", name);
    assert.ok(result.blockers.length, name);
  }
});
test("browser probe performs a real WebGL2 readback with no clock/FPS substitution", () => {
  const src = String(browserProbeSource());
  assert.match(src, /getContext\("webgl2"/);
  assert.match(src, /WEBGL_debug_renderer_info/);
  assert.match(src, /readPixels/);
  assert.doesNotMatch(src, /requestAnimationFrame|performance\.now/);
});
test("exclusive reports preserve both initial failure and successful evidence", async t => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "noon-readiness-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const outfile = path.join(dir, "result.json");
  let called = 0;
  const launch = async options => {
    called++;
    assert.equal(options.headless, false);
    assert.deepEqual(options.args, flags);
    return { version: () => EXPECTED.chromium,
      newContext: async () => ({ newPage: async () => ({ goto: async () => {},
        evaluate: async () => gl }), close: async () => {} }), close: async () => {} };
  };
  const result = await runReadiness({ launch, flags, outfile });
  assert.equal(called, 1);
  assert.equal(result.decision.performanceAcceptance, false);
  assert.deepEqual(JSON.parse(await readFile(outfile, "utf8")), result);
  await assert.rejects(runReadiness({ launch, flags, outfile }), { code: "EEXIST" });
});
test("invalid browser flags fail closed and still preserve the report", async t => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "noon-readiness-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const outfile = path.join(dir, "result.json");
  const result = await runReadiness({
    launch: async () => { throw new Error("must not launch"); },
    flags: [...flags, "--use-angle=swiftshader"], outfile,
  });
  assert.equal(result.decision.status, "blocked");
  assert.match(result.errors[0], /hardware flags must be exactly/);
});
