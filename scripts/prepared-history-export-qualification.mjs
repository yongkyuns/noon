// Exercise the canonical Python export CLI and the explicit native media importer.
// This host never supplies a scene, sample clock, replay state or audio mixer.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { copyFile, mkdir, readFile, rm, stat, writeFile } from "node:fs/promises";
import { finished } from "node:stream/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const usage = `Usage: node scripts/prepared-history-export-qualification.mjs
  --output NEW_DIRECTORY --encoder ENCODE_PREPARED_CAPTURE
  --music-renderer RENDER_BACKGROUND_MUSIC [--python PYTHON]
  [--native-video PAIRED_MINIMAL.mp4]
  [--backends webgl,webgpu] [--transports transferable,shared]
Requires a freshly built web package and the named native binaries.
NOON_EXPORT_CHROMIUM and NOON_PYODIDE_ROOT use the canonical export CLI's
explicit browser and pinned local resource options. Existing outputs are kept.
After each profile passes, bulky raw media is removed; set
NOON_PREPARED_HISTORY_EXPORT_KEEP_RAW=1 to preserve it for diagnosis.`;
const options = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const name = process.argv[i];
  if (["--help", "-h"].includes(name)) { console.log(usage); process.exit(0); }
  assert.ok(["--output", "--encoder", "--music-renderer", "--python", "--backends", "--transports", "--native-video"].includes(name), usage);
  assert.ok(process.argv[i + 1] && !options[name], `missing or duplicate ${name}`);
  options[name] = process.argv[i + 1];
}
for (const name of ["--output", "--encoder", "--music-renderer"]) assert.ok(options[name], usage);
const output = path.resolve(options["--output"]);
const encoder = path.resolve(options["--encoder"]);
const musicRenderer = path.resolve(options["--music-renderer"]);
const nativeVideo = options["--native-video"] ? path.resolve(options["--native-video"]) : null;
const python = options["--python"] ?? process.env.NOON_PYTHON ?? "python3";
const backends = (options["--backends"] ?? process.env.NOON_PREPARED_HISTORY_EXPORT_BACKENDS ?? "webgl,webgpu").split(",");
const transports = (options["--transports"] ?? process.env.NOON_PREPARED_HISTORY_EXPORT_TRANSPORTS ?? "transferable,shared").split(",");
assert.ok(backends.length && backends.every(x => ["webgl", "webgpu"].includes(x)) && new Set(backends).size === backends.length);
assert.ok(transports.length && transports.every(x => ["transferable", "shared"].includes(x)) && new Set(transports).size === transports.length);
const keepRaw = process.env.NOON_PREPARED_HISTORY_EXPORT_KEEP_RAW === "1";
const hash = value => createHash("sha256").update(value).digest("hex");
const json = value => JSON.stringify(value, null, 2) + "\n";
async function fileHash(filename) {
  const digest = createHash("sha256");
  for await (const chunk of createReadStream(filename)) digest.update(chunk);
  return digest.digest("hex");
}
async function exists(filename) {
  try { await stat(filename); return true; }
  catch (error) { if (error.code === "ENOENT") return false; throw error; }
}

const fixtures = path.join(root, "tests/fixtures/fourier_audio");
const originalHashes = {
  "audio_ordinary_play.py": "a898089e0534e31a5a149daaff4fec95ffc7ba5d108125d8ba93b4e252e5a60b",
  "fourier_epicycles.py": "7a09cda6039efdc9901a3241b0c514f634996187a0ea9b1f316e34dbbd1feed9",
};
const originals = {};
for (const [name, expected] of Object.entries(originalHashes)) {
  originals[name] = await readFile(path.join(fixtures, name), "utf8");
  assert.equal(hash(originals[name]), expected, `preserve original source ${name}`);
}
const insertion = "        self.audio(AudioSource.small_discovery(), duration=34.65, gain=0.5, fade_in=1.5, fade_out=2.5)\n";
const firstPlay = "        self.play(FadeIn(title)";
assert.equal(originals["fourier_epicycles.py"].split(firstPlay).length, 2);
const fourier = originals["fourier_epicycles.py"].replace(firstPlay, insertion + firstPlay);
assert.equal(fourier.replace(insertion, ""), originals["fourier_epicycles.py"]);
assert.equal(hash(fourier), "9a2ba12dde3ad3941cdde00ba9eaa3c0cde82ec2869a85b6dda38aeb3b19c6ff");

// Fail before creating an output when required build inputs are absent.
for (const filename of [encoder, musicRenderer, path.join(root, "web/pkg/noon_web_bg.wasm"), ...(nativeVideo ? [nativeVideo] : [])]) {
  assert.ok((await stat(filename)).isFile(), `missing build input ${filename}`);
}
await mkdir(output); // Deliberately requires a new directory.
await mkdir(path.join(output, "logs"));
const generatedSource = path.join(output, "combined_fourier_audio.py");
await writeFile(generatedSource, fourier, { flag: "wx" });
await writeFile(path.join(output, "audio_ordinary_play.py"), originals["audio_ordinary_play.py"], { flag: "wx" });
const report = {
  status: "running", started_at: new Date().toISOString(),
  scope: "completed ordinary Python history, retained browser rendering, shared PCM, explicit native MP4 import and independent media decode; no physical-device or performance claim",
  revision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
  source_dirty: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim() !== "",
  node_version: process.version, original_sha256: originalHashes,
  combined_fourier_sha256: hash(fourier), commands: [], cases: [], profiles: [],
  backends, transports, keep_raw: keepRaw,
  inputs: {},
};
const save = () => writeFile(path.join(output, "qualification.json"), json(report));
await save();

async function run(name, executable, args, { acceptFailure = false, cancelAfterFrame = null, timeout = 900_000 } = {}) {
  const stdoutPath = path.join(output, "logs", `${name}.stdout.log`);
  const stderrPath = path.join(output, "logs", `${name}.stderr.log`);
  const stdoutFile = createWriteStream(stdoutPath, { flags: "wx" });
  const stderrFile = createWriteStream(stderrPath, { flags: "wx" });
  const command = { name, executable, args, started_at: new Date().toISOString() };
  report.commands.push(command);
  await save();
  console.log(`[RUN] ${name}`);
  const child = spawn(executable, args, { cwd: root, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.pipe(stdoutFile);
  child.stderr.pipe(stderrFile);
  let stdout = "", failure = null, sentCancellation = false, exited = false, forceTimer = null;
  const terminate = () => {
    child.kill("SIGTERM");
    forceTimer ??= setTimeout(() => {
      if (exited) return;
      failure ??= new Error(`${name}: child failed to finish teardown after cancellation`);
      child.kill("SIGKILL");
    }, 15_000);
  };
  child.stdout.on("data", chunk => {
    stdout += chunk;
    if (stdout.length > 2 * 1024 * 1024) {
      failure = new Error(`${name}: command summary exceeds 2 MiB`);
      terminate();
    }
  });
  child.stderr.on("data", chunk => {
    if (/Captured \d+\//.test(String(chunk))) process.stdout.write(`[${name}] ${chunk}`);
  });
  const timer = setTimeout(() => {
    failure = new Error(`${name}: timed out`);
    terminate();
  }, timeout);
  const progress = setInterval(() => console.log(`[RUNNING] ${name}`), 30_000);
  let inspecting = false;
  const cancelTimer = cancelAfterFrame && setInterval(async () => {
    if (inspecting || exited || sentCancellation) return;
    inspecting = true;
    try {
      if ((await stat(cancelAfterFrame)).size >= 640 * 360 * 4) {
        sentCancellation = true;
        command.cancelled_after_captured_frame = true;
        terminate();
      }
    } catch (error) {
      if (error.code !== "ENOENT") { failure = error; terminate(); }
    } finally { inspecting = false; }
  }, 20);
  try {
    await new Promise(resolve => {
      child.once("error", error => { failure = error; });
      child.once("close", (code, signal) => {
        exited = true; command.exit_code = code; command.signal = signal; resolve();
      });
    });
  } finally {
    clearTimeout(timer); clearTimeout(forceTimer); clearInterval(progress); if (cancelTimer) clearInterval(cancelTimer);
    await Promise.all([finished(stdoutFile), finished(stderrFile)]);
    command.finished_at = new Date().toISOString();
    command.stdout_sha256 = await fileHash(stdoutPath);
    command.stderr_sha256 = await fileHash(stderrPath);
    if (failure) command.error = String(failure.stack ?? failure);
    await save();
  }
  if (failure) throw failure;
  if (cancelAfterFrame) assert.ok(sentCancellation, `${name}: source finished before cancellation observation`);
  if (!acceptFailure) assert.equal(command.exit_code, 0, `${name}: inspect ${stderrPath}`);
  else assert.notEqual(command.exit_code, 0, `${name}: unexpectedly succeeded`);
  return { stdout, command };
}

function cliArgs(source, capture, movie, backend, transport, extra = []) {
  return [path.join(root, "scripts/export-python-scene.mjs"), source, capture,
    "--mp4", movie, "--encoder", encoder, "--width", "640", "--height", "360",
    "--fps", "30000/1001", "--backend", backend, "--transport", transport,
    "--max-frames", "1100", ...extra];
}

async function typedWasmProof() {
  const server = await serveRepository(root, 0, { crossOriginIsolated: true });
  let browser;
  const errors = [];
  const value = {
    scope: "direct typed Rust/WASM completed reader, shared frame policy and mixer; publication consumer proof, no GPU pixel claim",
    browser_errors: errors,
  };
  report.typed_rust_wasm = value;
  try {
    browser = await playwright.chromium.launch({
      ...(process.env.NOON_EXPORT_CHROMIUM ? { executablePath: process.env.NOON_EXPORT_CHROMIUM } : { channel: "chromium" }),
      headless: true, args: browserArgs(backends[0]),
    });
    const page = await browser.newPage();
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
    let timer, observed;
    try {
      observed = await Promise.race([page.evaluate(async () => {
        const { loadRuntimeBuild } = await import("./runtime-build-verifier.js");
        const build = await loadRuntimeBuild();
        const module = await import("./pkg/noon_web.js");
        await module.default({ module_or_path: build.wasmBytes });
        return { runtime_build_identity: build.identity, result: JSON.parse(module.verifyCompletedHistoryExport()) };
      }), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("typed Rust/WASM export proof timed out")), 120_000);
      })]);
    } finally { clearTimeout(timer); }
    Object.assign(value, observed, { browser_version: browser.version() });
  } finally {
    const results = await Promise.allSettled([browser?.close(), server.close()]);
    for (const result of results) if (result.status === "rejected") errors.push(String(result.reason));
    await save();
  }
  assert.deepEqual(errors, [], "typed Rust/WASM errors including teardown");
  const result = value.result;
  assert.equal(result.profile, "completed_prepared_history_rust_wasm");
  for (const [key, expected] of Object.entries({ source_calls_before: 3, source_calls_after: 3,
    source_drops_before_export: 1, source_end: 3, final_time: 3, prepared_time: 0, frames: 90,
    observations: 91, execution_revisions_advanced: false, frame_epochs_advanced: true, final_objects: 1,
    audio_frames: 144144, silent_padding_frames: 144 })) assert.equal(result[key], expected, key);
  assert.equal(result.runtime_before, result.runtime_after);
  assert.ok(result.pcm_peak > 0 && result.pcm_energy > 0 && /^[0-9a-f]{16}$/.test(result.pcm_checksum));
}

try {
  for (const filename of [encoder, musicRenderer, fileURLToPath(import.meta.url),
    path.join(root, "scripts/verify-prepared-history-export.py"),
    path.join(root, "scripts/export-python-scene.mjs"), path.join(root, "web/pkg/noon_web_bg.wasm"),
    path.join(root, "web/pkg/noon_web.js"), path.join(root, "web/python-worker.js"),
    path.join(root, "web/runtime-build-identity.json")]) {
    report.inputs[filename] = { bytes: (await stat(filename)).size, sha256: await fileHash(filename) };
  }
  await typedWasmProof();
  const unmixed = path.join(output, "unmixed-music-24000.wav");
  await run("unmixed-music", musicRenderer, [unmixed, "24000"]);
  report.inputs[unmixed] = { bytes: (await stat(unmixed)).size, sha256: await fileHash(unmixed) };
  const immutableDirectory = path.join(output, "immutable-three");
  await mkdir(immutableDirectory);
  const immutableCapture = path.join(immutableDirectory, "immutable-three-capture");
  const immutableResult = await run("immutable-three", process.execPath,
    cliArgs(path.join(root, "web/python/examples/background_music_lesson.py"), immutableCapture,
      path.join(immutableDirectory, "immutable-three.mp4"), backends[0], transports[0], ["--frames", "3"]));
  const immutableReport = JSON.parse(immutableResult.stdout);
  assert.equal(immutableReport.capture_source.kind, "immutable");
  await writeFile(path.join(immutableDirectory, "immutable-three-report.json"), json(immutableReport), { flag: "wx" });
  await run("immutable-three-verification", python,
    [path.join(root, "scripts/verify-prepared-history-export.py"), immutableDirectory, "--music-wav", unmixed, "--immutable"]);
  report.immutable_regression = JSON.parse(await readFile(path.join(immutableDirectory, "verification.json"), "utf8"));
  assert.equal(report.immutable_regression.status, "passed");
  if (!keepRaw) for (const filename of ["frames.rgba", "audio.f32le"]) await rm(path.join(immutableCapture, filename));
  const retainedNativeVideo = nativeVideo && path.join(output, "native-minimal.mp4");
  if (nativeVideo) {
    await copyFile(nativeVideo, retainedNativeVideo);
    report.inputs[nativeVideo] = { bytes: (await stat(nativeVideo)).size, sha256: await fileHash(nativeVideo) };
  }
  for (const backend of backends) for (const transport of transports) {
    const id = `${backend}-${transport}`, profileDirectory = path.join(output, id);
    await mkdir(profileDirectory);
    const profile = { id, backend, transport, status: "running", checks: [], cases: [] };
    report.profiles.push(profile);
    for (const [name, source, startFrame] of [
      ["minimal", path.join(fixtures, "audio_ordinary_play.py"), 0],
      ["fourier-full", generatedSource, 0],
      ["fourier-crop", generatedSource, 700],
    ]) {
      const capture = path.join(profileDirectory, `${name}-capture`), movie = path.join(profileDirectory, `${name}.mp4`);
      const result = await run(`${id}-${name}`, process.execPath, cliArgs(source, capture, movie, backend, transport,
        startFrame ? ["--start-frame", String(startFrame)] : []));
      const captured = JSON.parse(result.stdout);
      assert.equal(captured.capture_source.kind, "completed_history");
      assert.deepEqual(captured.browser_errors, []);
      const entry = { profile: id, name, start_frame: startFrame, capture, movie, result: captured };
      report.cases.push(entry); profile.cases.push(name);
      await writeFile(path.join(profileDirectory, `${name}-report.json`), json(captured), { flag: "wx" });
      await save();
    }
    // The existing MP4 must be rejected before preparation or capture directory creation.
    const minimalMovie = path.join(profileDirectory, "minimal.mp4");
    const before = await fileHash(minimalMovie), forbiddenCapture = path.join(profileDirectory, "no-clobber-capture");
    await run(`${id}-no-clobber`, process.execPath,
      cliArgs(path.join(fixtures, "audio_ordinary_play.py"), forbiddenCapture, minimalMovie, backend, transport), { acceptFailure: true });
    assert.equal(await fileHash(minimalMovie), before);
    assert.equal(await exists(forbiddenCapture), false);
    profile.checks.push("existing-movie-is-preserved-before-preparation");

    const limitedCapture = path.join(profileDirectory, "limited-capture");
    const limitedMovie = path.join(profileDirectory, "limited.mp4");
    const limitedArgs = cliArgs(path.join(fixtures, "audio_ordinary_play.py"), limitedCapture, limitedMovie, backend, transport);
    limitedArgs[limitedArgs.indexOf("--max-frames") + 1] = "50";
    await run(`${id}-preparation-limit`, process.execPath, limitedArgs, { acceptFailure: true });
    const limitError = await readFile(path.join(output, "logs", `${id}-preparation-limit.stderr.log`), "utf8");
    assert.match(limitError, /completed-history export source exceeds its max_frames preparation limit/);
    assert.equal(await exists(limitedCapture), false);
    assert.equal(await exists(limitedMovie), false);
    profile.checks.push("actual-Python-preparation-rejects-source-beyond-frame-cap");

    const cancelledCapture = path.join(profileDirectory, "cancelled-capture"), cancelledMovie = path.join(profileDirectory, "cancelled.mp4");
    await run(`${id}-cancel-during-capture`, process.execPath,
      cliArgs(path.join(fixtures, "audio_ordinary_play.py"), cancelledCapture, cancelledMovie, backend, transport), {
        acceptFailure: true, cancelAfterFrame: path.join(cancelledCapture, "frames.rgba"), timeout: 240_000,
      });
    assert.equal(await exists(cancelledCapture), false);
    assert.equal(await exists(cancelledMovie), false);
    profile.checks.push("cancellation-removes-incomplete-capture-and-publishes-no-movie");

    // Failure at the explicit encoder boundary preserves a completed importable
    // capture, without replacing an existing movie even with overwrite requested.
    const failedCapture = path.join(profileDirectory, "encoder-failure-capture");
    const failedArgs = cliArgs(path.join(fixtures, "audio_ordinary_play.py"), failedCapture, minimalMovie, backend, transport,
      ["--overwrite"]);
    failedArgs[failedArgs.indexOf("--encoder") + 1] = path.join(output, "absent-encoder");
    await run(`${id}-encoder-failure`, process.execPath, failedArgs, { acceptFailure: true });
    assert.equal(await fileHash(minimalMovie), before);
    assert.equal(await exists(path.join(failedCapture, "capture.json")), true);
    for (const filename of ["frames.rgba", "audio.f32le"]) {
      assert.equal(await fileHash(path.join(failedCapture, filename)),
        await fileHash(path.join(profileDirectory, "minimal-capture", filename)), "failed encode retains the same complete minimal media");
    }
    profile.checks.push("encoder-failure-preserves-complete-capture-and-existing-movie");

    await run(`${id}-independent-verification`, python,
      [path.join(root, "scripts/verify-prepared-history-export.py"), profileDirectory, "--music-wav", unmixed,
        ...(retainedNativeVideo ? ["--native-video", retainedNativeVideo] : [])]);
    profile.verification = JSON.parse(await readFile(path.join(profileDirectory, "verification.json"), "utf8"));
    assert.equal(profile.verification.status, "passed");
    if (!keepRaw) {
      for (const name of ["minimal", "fourier-full", "fourier-crop", "encoder-failure"]) {
        for (const filename of ["frames.rgba", "audio.f32le"]) await rm(path.join(profileDirectory, `${name}-capture`, filename));
      }
      profile.raw_removed_after_verification = true;
    }
    profile.status = "passed";
    await save();
  }
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = String(error.stack ?? error);
  process.exitCode = 1;
} finally {
  report.finished_at = new Date().toISOString();
  await save();
}
console.log(json({ status: report.status, output, cases: report.cases.map(value => value.name), error: report.error }));
