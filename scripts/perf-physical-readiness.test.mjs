import assert from "node:assert/strict";
import { chmod, copyFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { EXPECTED, powerSource, classifyWebgl, assessReadiness, browserProbeSource,
  runReadiness, parseBuildProcesses } from "./perf-physical-readiness.mjs";

const flags = ["--disable-features=WebGPU", "--use-gpu-in-tests", "--ignore-gpu-blocklist",
  "--force-high-performance-gpu", "--disable-gpu-sandbox", "--disable-dev-shm-usage"];
const gl = { backend: "WebGL2", unmaskedVendor: "Apple Inc.",
  unmaskedRenderer: "ANGLE (Apple, Apple M4, Metal)", readbackValid: true,
  contextLost: false };
const ok = { platform: "darwin", node: "v22.23.3", browser: "151.0.7922.34",
  cores: 10, power: "ac", webgl: gl, buildProcesses: [] };

test("build activity distinguishes executable names and rejects missing/malformed evidence", () => {
  assert.deepEqual(parseBuildProcesses("1 /usr/bin/node\n2 /opt/my app/bin/rustc\n3 cargo\n4 xtensa-esp-elf-ld\n5 cargo-helper"),
    [{ pid: 2, executable: "rustc" }, { pid: 3, executable: "cargo" }, { pid: 4, executable: "xtensa-esp-elf-ld" }]);
  assert.deepEqual(parseBuildProcesses("1 /usr/bin/node\n2 /opt/app/Codex"), []);
  assert.deepEqual(parseBuildProcesses("1 arm-none-eabi-gcc\n2 clang-18\n3 gmake"),
    [{ pid: 1, executable: "arm-none-eabi-gcc" }, { pid: 2, executable: "clang-18" }, { pid: 3, executable: "gmake" }]);
  for (const executable of ["swift", "swiftc", "swift-build", "swift-driver", "swift-frontend", "xcodebuild"]) {
    const processes = parseBuildProcesses(`1 /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/${executable}\n2 node`);
    assert.deepEqual(processes, [{ pid: 1, executable }]);
    assert.equal(assessReadiness({ ...ok, buildProcesses: processes }).status, "blocked");
  }
  assert.deepEqual(parseBuildProcesses("1 swift-driver-helper\n2 my-xcodebuild\n3 /opt/Swift.app/app"), []);
  for (const value of [null, "", "PID COMMAND", "0 rustc", "9007199254740992 rustc", "1 node\nbroken"]) {
    assert.equal(parseBuildProcesses(value), null);
  }
});

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
    ["buildProcesses", null], ["buildProcesses", [{ pid: 123, executable: "rustc" }]],
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

for (const compiler of ["rustc", "swift-driver", "swift-frontend", "xcodebuild"]) test(`device capture refuses ${compiler} and preserves reports if it starts during a suite`, async t => {
  const dir = await mkdtemp(path.join(os.tmpdir(), "noon-device-builds-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  await mkdir(path.join(dir, "scripts")); await mkdir(path.join(dir, "bin"));
  for (const file of ["perf-device-run.mjs", "perf-physical-readiness.mjs"]) {
    await copyFile(new URL(file, import.meta.url), path.join(dir, "scripts", file));
  }
  const marker = path.join(dir, "building");
  const ps = path.join(dir, "bin", "ps");
  await writeFile(ps, `#!${process.execPath}\nconst fs=require('node:fs');process.stdout.write(fs.existsSync(${JSON.stringify(marker)})?'123 /tmp/other project/bin/${compiler}\\n':'123 node\\n');\n`);
  await chmod(ps, 0o755);
  await writeFile(path.join(dir, "scripts", "perf-corpus.mjs"), `
    import fs from 'node:fs';
    fs.writeFileSync(process.env.NOON_CORPUS_ARTIFACT, 'raw evidence');
    fs.writeFileSync(${JSON.stringify(marker)}, 'active');
  `);
  const capture = (out, gpuMode = "hardware") => spawnSync(process.execPath, [path.join(dir, "scripts/perf-device-run.mjs")], {
    cwd: dir, encoding: "utf8", env: { ...process.env, PATH: `${path.join(dir, "bin")}:${process.env.PATH}`,
      NOON_PERF_DEVICE_ID: "test-device", NOON_PERF_GPU_MODE: gpuMode,
      NOON_PERF_BACKENDS: "webgpu", NOON_PERF_SUITES: "corpus", NOON_PERF_RUN_DIR: out },
  });
  await writeFile(marker, "active");
  const before = path.join(dir, "before");
  let result = capture(before);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /compiler\/build\/archive tooling is active/);
  assert.deepEqual(await readdir(before), ["perf-corpus-webgpu-builds-before.json"]);
  const read = (out, phase) => readFile(path.join(out, `perf-corpus-webgpu-builds-${phase}.json`), "utf8").then(JSON.parse);
  assert.deepEqual((await read(before, "before")).processes, [{ pid: 123, executable: compiler }]);
  await rm(marker);
  const after = path.join(dir, "after"); result = capture(after);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /compiler\/build\/archive tooling is active/);
  assert.deepEqual((await read(after, "before")).processes, []);
  assert.deepEqual((await read(after, "after")).processes, [{ pid: 123, executable: compiler }]);
  assert.equal(await readFile(path.join(after, "corpus-webgpu.json"), "utf8"), "raw evidence");
  const original = await readFile(path.join(after, "perf-corpus-webgpu-builds-before.json"), "utf8");
  result = capture(after);
  assert.notEqual(result.status, 0); assert.match(result.stderr, /EEXIST/);
  assert.equal(await readFile(path.join(after, "perf-corpus-webgpu-builds-before.json"), "utf8"), original);
  for (const [name, program] of [["malformed", "process.stdout.write('invalid\\n');"], ["unavailable", "process.exit(1);"]]) {
    await writeFile(ps, `#!${process.execPath}\n${program}\n`);
    const out = path.join(dir, name); result = capture(out);
    assert.notEqual(result.status, 0); assert.match(result.stderr, /host build activity unknown/);
    assert.equal((await read(out, "before")).processes, null);
    assert.deepEqual(await readdir(out), ["perf-corpus-webgpu-builds-before.json"]);
  }
  // Software diagnostics do not acquire a physical-host verdict or depend on ps.
  const software = path.join(dir, "software");
  result = capture(software, "software");
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(await readFile(path.join(software, "manifest.json"), "utf8")).buildActivityChecks, []);
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
test("self-hosted physical job is manual and default-branch-only", async () => {
  const workflow = await readFile(new URL("../.github/workflows/perf-physical-readiness.yml", import.meta.url), "utf8");
  const physical = workflow.split("\n  physical:\n")[1];
  assert.ok(physical, "physical job must be explicit");
  assert.match(physical, /^    runs-on: \[self-hosted, macOS, noon-physical\]$/m);
  assert.match(physical, /^    if: github.event_name == 'workflow_dispatch' && github.ref == 'refs\/heads\/master'$/m);
  assert.match(workflow, /^  pull_request:$/m);
  assert.match(workflow, /^  workflow_dispatch:$/m);
  assert.match(workflow, /Readiness protocol tests \(hosted, no GPU verdict\)/);
});
