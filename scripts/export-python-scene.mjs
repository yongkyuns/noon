// Explicit export media from the normal Python/worker/runtime/renderer path.
// Rust selects samples and mixes PCM; this host only waits, copies and writes.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, open, readFile, rm, stat, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { browserArgs } from "./manim-raster-support.mjs";
import { createPyodideResourceCache } from "./pyodide-resource-cache.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const USAGE = `Usage: node scripts/export-python-scene.mjs SOURCE.py CAPTURE_DIRECTORY [options]
  --mp4 OUTPUT.mp4       Finalize the capture through the native MP4 sink
  --encoder PATH        Built encode_prepared_capture binary (otherwise cargo run)
  --overwrite           Explicitly replace an existing MP4 after successful export
  --width N --height N  Even output dimensions (default 640 x 360)
  --fps NUM/DEN         Rational frame rate (default 30/1)
  --start-frame N       Crop on the source's zero-origin grid (default 0)
  --frames N            Stop after at most N output frames
  --end-time SECONDS    Exclusive authored end, instead of --frames
  --max-frames N        Safety cap including discarded prefix (default 18000)
  --hold SECONDS        Frozen terminal hold (default 0)
  --backend webgpu|webgl  Chromium software renderer (default webgpu)
  --transport transferable|shared  Worker transport (default transferable)
The Python source may author a finite immutable scene or an ordinary fixed-audio
play/wait scene that can prepare deterministic history. The same invocation
prepares silently to a bounded terminal endpoint before export. Arbitrary Python
callbacks and other realtime source continuations are outside this profile.
CAPTURE_DIRECTORY must be new.
NOON_EXPORT_CHROMIUM may select a Chromium executable. NOON_PYODIDE_ROOT may
supply local bytes for the production worker's pinned Pyodide release.`;

function integer(value, name, minimum = 0) {
  const number = Number(value);
  if (!/^\d+$/.test(String(value)) || !Number.isSafeInteger(number) || number < minimum) {
    throw new Error(`${name} must be a safe integer at least ${minimum}`);
  }
  return number;
}

function nonnegative(value, name) {
  const number = Number(value);
  if (!Number.isFinite(number) || number < 0 || String(value).trim() === "") {
    throw new Error(`${name} must be a finite nonnegative number`);
  }
  return number;
}

function argumentsFor(argv) {
  if (argv.length === 1 && ["--help", "-h"].includes(argv[0])) return null;
  if (argv.length < 2 || argv[0].startsWith("--") || argv[1].startsWith("--")) {
    throw new Error(USAGE);
  }
  const config = {
    sourcePath: path.resolve(argv[0]), directory: path.resolve(argv[1]),
    width: 640, height: 360, backend: "webgpu", transport: "transferable",
    output: null, encoder: null, overwrite: false,
    frameOptions: {
      frame_rate: { numerator: 30, denominator: 1 }, start_frame: 0,
      stop: { kind: "source_end" }, max_frames: 18_000,
      max_transitions_per_sample: 64, final_hold_seconds: 0,
    },
  };
  let stopped = false;
  const seen = new Set();
  for (let index = 2; index < argv.length; index++) {
    const flag = argv[index];
    if (seen.has(flag)) throw new Error(`duplicate option ${flag}`);
    seen.add(flag);
    if (flag === "--overwrite") { config.overwrite = true; continue; }
    const value = argv[++index];
    if (value === undefined) throw new Error(`missing value for ${flag}`);
    switch (flag) {
      case "--mp4": config.output = path.resolve(value); break;
      case "--encoder": config.encoder = path.resolve(value); break;
      case "--width": config.width = integer(value, flag, 2); break;
      case "--height": config.height = integer(value, flag, 2); break;
      case "--start-frame": config.frameOptions.start_frame = integer(value, flag); break;
      case "--max-frames": config.frameOptions.max_frames = integer(value, flag, 1); break;
      case "--hold": config.frameOptions.final_hold_seconds = nonnegative(value, flag); break;
      case "--fps": {
        const parts = value.split("/");
        if (parts.length !== 2) throw new Error("--fps must have NUM/DEN form");
        const numerator = integer(parts[0], flag, 1), denominator = integer(parts[1], flag, 1);
        if (numerator > 2_147_483_647 || denominator > 2_147_483_647) throw new Error("--fps exceeds the native encoder range");
        config.frameOptions.frame_rate = { numerator, denominator };
        break;
      }
      case "--frames":
      case "--end-time":
        if (stopped) throw new Error("choose only one of --frames and --end-time");
        stopped = true;
        config.frameOptions.stop = flag === "--frames"
          ? { kind: "frame_count", frames: integer(value, flag, 1) }
          : { kind: "end_time", time: nonnegative(value, flag) };
        break;
      case "--backend":
        if (!["webgpu", "webgl"].includes(value)) throw new Error("invalid renderer backend");
        config.backend = value;
        break;
      case "--transport":
        if (!["transferable", "shared"].includes(value)) throw new Error("invalid worker transport");
        config.transport = value;
        break;
      default: throw new Error(`unknown option ${flag}\n${USAGE}`);
    }
  }
  if (config.width % 2 || config.height % 2 || config.width * config.height * 4 > 256 * 1024 * 1024) {
    throw new Error("output dimensions must be even and fit the 256 MiB frame budget");
  }
  if ((config.overwrite || config.encoder !== null) && config.output === null) {
    throw new Error("--overwrite and --encoder require --mp4");
  }
  return config;
}

async function deadline(operation, label, milliseconds = 30_000) {
  let timer;
  try {
    return await Promise.race([
      operation,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), milliseconds); }),
    ]);
  } finally { clearTimeout(timer); }
}

async function requireNew(pathname) {
  try { await stat(pathname); }
  catch (error) { if (error.code === "ENOENT") return; throw error; }
  throw new Error(`destination already exists: ${pathname}`);
}

async function run(config) {
  const sourceInfo = await stat(config.sourcePath);
  if (!sourceInfo.isFile() || sourceInfo.size > 8 * 1024 * 1024) throw new Error("Python source must be a regular file within 8 MiB");
  const source = await readFile(config.sourcePath, "utf8");
  await requireNew(config.directory);
  if (config.output !== null && !config.overwrite) await requireNew(config.output);
  const sourceHash = createHash("sha256").update(source).digest("hex");
  const workerSource = await readFile(path.join(repoRoot, "web/python-worker.source.js"), "utf8");
  const resourceCache = createPyodideResourceCache(workerSource);
  const localResources = {};
  const executablePath = process.env.NOON_EXPORT_CHROMIUM;
  const browserExecutableSha256 = executablePath
    ? createHash("sha256").update(await readFile(executablePath)).digest("hex") : null;
  const server = await serveRepository(repoRoot, 0, { crossOriginIsolated: true });
  let browser, page, framesFile, pcmFile, directoryOwned = false, captureComplete = false;
  const errors = [];
  let encoderProcess = null, interrupted = false, serverOpen = true, failure = null;
  const checkInterrupted = () => {
    if (interrupted) throw new Error("Python capture cancelled");
  };
  const interrupt = () => {
    interrupted = true;
    if (browser) void browser.close().catch(error => errors.push(`cancelled browser shutdown: ${error}`));
    encoderProcess?.kill("SIGTERM");
  };
  process.once("SIGINT", interrupt);
  process.once("SIGTERM", interrupt);
  try {
    await mkdir(config.directory);
    directoryOwned = true;
    checkInterrupted();
    browser = await playwright.chromium.launch({
      ...(executablePath ? { executablePath } : { channel: "chromium" }),
      headless: true, args: browserArgs(config.backend),
    });
    checkInterrupted();
    const context = await browser.newContext({ viewport: { width: Math.max(config.width, 640), height: Math.max(config.height, 360) }, deviceScaleFactor: 1 });
    await resourceCache.install(context);
    if (process.env.NOON_PYODIDE_ROOT) {
      const base = resourceCache.stats().baseUrl;
      await context.route(url => url.href.startsWith(base), async route => {
        if (route.request().method() !== "GET") return route.fallback();
        const name = new URL(route.request().url()).pathname.slice(new URL(base).pathname.length);
        if (!name || path.basename(name) !== name) return route.fallback();
        try {
          const bytes = await readFile(path.join(process.env.NOON_PYODIDE_ROOT, name));
          localResources[name] = { bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
          await route.fulfill({ status: 200, body: bytes, headers: {
            "access-control-allow-origin": "*", "cross-origin-resource-policy": "cross-origin",
            "content-type": name.endsWith(".mjs") ? "text/javascript" :
              name.endsWith(".wasm") ? "application/wasm" :
                name.endsWith(".json") ? "application/json" : "application/octet-stream",
          } });
        } catch (error) {
          if (error.code === "ENOENT") return route.fallback();
          errors.push(`Pyodide resource ${name}: ${error.message}`);
          await route.abort("failed").catch(() => {});
        }
      });
    }
    page = await context.newPage();
    page.on("pageerror", error => errors.push(String(error)));
    page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
    page.on("requestfailed", request => errors.push(`request failed: ${request.url()} (${request.failure()?.errorText})`));
    page.on("response", response => { if (response.status() >= 400) errors.push(`HTTP ${response.status()}: ${response.url()}`); });
    await page.goto(`${server.baseUrl}/web/execution-worker-smoke.html`, { waitUntil: "load" });
    const start = await deadline(page.evaluate(async ({ source, width, height, frameOptions, transport }) => {
      const { PythonAuthoringClient } = await import("./authoring-client.js");
      const { AuthoringExecutionClient } = await import("./authoring-execution-client.js");
      const canvas = document.querySelector("#scene");
      canvas.width = width; canvas.height = height;
      canvas.style.width = `${width}px`; canvas.style.height = `${height}px`;
      const authoring = new PythonAuthoringClient();
      const execution = new AuthoringExecutionClient(canvas, { onError: error => { throw error; } });
      window.noonFiniteCapture = { authoring, execution };
      const runtimeBuildIdentity = await authoring.ready();
      let preparation = null;
      const authored = await authoring.run(source, {}, {
        async onSemanticContinuation(registration) {
          if (registration.semanticExecution.preparedPlayback !== true) {
            throw new Error("export requires finite immutable authoring or deterministic fixed-audio prepared history; realtime source continuations are unsupported");
          }
          await execution.startSemanticExecution(registration.semanticExecution, {
            authoringClient: authoring, initiallyPaused: true, transportMode: transport,
            loopDurationSeconds: Math.max(registration.duration, 1),
          });
          // Rust owns the bounded demand and terminal admission. Resume this
          // same suspended source invocation without moving the displayed reader.
          preparation = await execution.prepareHistoryExport(frameOptions);
          if (preparation.kind !== "preparing_history") {
            throw new Error("prepared history export intent was not admitted");
          }
        },
      });
      if (preparation === null) {
        if (!authored.semanticExecution || authored.semanticExecution.continuationGeneration !== undefined) {
          throw new Error("finite export did not receive a complete immutable Python scene");
        }
        await execution.startSemanticExecution(authored.semanticExecution, {
          authoringClient: authoring, initiallyPaused: true, transportMode: transport,
          loopDurationSeconds: Math.max(authored.duration, 1),
        });
      }
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const plan = await execution.beginFiniteExport(frameOptions);
      if (preparation === null && authored.duration > plan.source_end) {
        throw new Error(`finite export does not yet support a trailing authored interval beyond retained tracks/audio (${authored.duration} > ${plan.source_end} seconds)`);
      }
      const copy = document.createElement("canvas");
      copy.width = width; copy.height = height;
      const context = copy.getContext("2d", { willReadFrequently: true, alpha: false });
      if (context === null) throw new Error("RGBA readback canvas is unavailable");
      window.noonFiniteCapture.copy = copy;
      window.noonFiniteCapture.context = context;
      window.noonFiniteCapture.base64 = bytes => {
        let encoded = "";
        for (let first = 0; first < bytes.length; first += 32_768) {
          encoded += String.fromCharCode(...bytes.subarray(first, first + 32_768));
        }
        return btoa(encoded);
      };
      return { plan, rendererBackend: execution.rendererBackend, transportMode: execution.transportMode,
        authoredDuration: authored.duration, runtimeBuildIdentity, preparation };
    }, { source, width: config.width, height: config.height, frameOptions: config.frameOptions, transport: config.transport }), "Python capture initialization", 180_000);
    assert.equal(start.rendererBackend, config.backend === "webgpu" ? "WebGPU" : "WebGL2");
    assert.equal(start.transportMode, config.transport);
    const plan = start.plan;
    checkInterrupted();
    assert.ok(Number.isSafeInteger(plan.sampling.frames) && plan.sampling.frames > 0);
    const frameBytes = config.width * config.height * 4;
    const audioFrames = plan.audio ? plan.audio_frames : 0;
    assert.ok(Number.isSafeInteger(audioFrames) && audioFrames >= 0);
    const mediaBytes = plan.sampling.frames * frameBytes + audioFrames * 8;
    assert.ok(Number.isSafeInteger(mediaBytes) && mediaBytes <= 64 * 1024 * 1024 * 1024, "capture exceeds 64 GiB media budget");
    framesFile = await open(path.join(config.directory, "frames.rgba"), "wx");
    let status = plan.status, frames = 0, manifestBytes = 1024;
    const samples = [];
    while (status.kind !== "complete") {
      checkInterrupted();
      assert.equal(status.kind, "needs_sample");
      const sample = await deadline(page.evaluate(() => window.noonFiniteCapture.execution.sampleFiniteExport()), "renderer sample");
      assert.equal(sample.kind, "sample");
      const observation = sample.observation;
      manifestBytes += Buffer.byteLength(JSON.stringify(observation)) + 1;
      assert.ok(manifestBytes <= 16 * 1024 * 1024, "capture exceeds 16 MiB metadata budget");
      samples.push(observation);
      if (sample.frame !== null) {
        assert.equal(sample.frame.pts, frames);
        const base64 = await deadline(page.evaluate(async ({ width, height }) => {
          // The worker command already waited for the matching retained
          // presentation. Let its OffscreenCanvas placeholder reach the page,
          // then synchronously copy pixels before acknowledging that sample.
          await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          const { execution, context, base64 } = window.noonFiniteCapture;
          if (execution.canvas.width !== width || execution.canvas.height !== height) {
            throw new Error("renderer dimensions changed during finite capture");
          }
          context.drawImage(execution.canvas, 0, 0);
          return base64(context.getImageData(0, 0, width, height).data);
        }, { width: config.width, height: config.height }), "RGBA readback");
        const rgba = Buffer.from(base64, "base64");
        assert.equal(rgba.length, frameBytes);
        await framesFile.writeFile(rgba);
        frames += 1;
        if (frames % Math.max(1, Math.ceil(plan.sampling.frames / 10)) === 0) {
          console.error(`Captured ${frames}/${plan.sampling.frames} frames`);
        }
      }
      assert.deepEqual(errors, [], "browser capture errors");
      status = await deadline(page.evaluate(observation => window.noonFiniteCapture.execution.acknowledgeFiniteExport(observation), observation), "sample acknowledgement");
    }
    assert.equal(frames, plan.sampling.frames);
    assert.deepEqual(status.sampling, plan.sampling);
    await framesFile.close(); framesFile = null;
    if (plan.audio) {
      pcmFile = await open(path.join(config.directory, "audio.f32le"), "wx");
      assert.ok(Number.isSafeInteger(plan.audio_block_frames) && plan.audio_block_frames > 0 && plan.audio_block_frames <= 16_384);
      for (let first = 0; first < audioFrames;) {
        checkInterrupted();
        const count = Math.min(plan.audio_block_frames, audioFrames - first);
        const base64 = await deadline(page.evaluate(async ({ first, count }) => {
          const samples = await window.noonFiniteCapture.execution.finiteExportAudioBlock(first, count);
          if (!(samples instanceof Float32Array) || samples.length !== count * 2) {
            throw new Error("shared mixer returned an invalid PCM block");
          }
          const bytes = new Uint8Array(samples.length * 4), view = new DataView(bytes.buffer);
          for (let index = 0; index < samples.length; index++) view.setFloat32(index * 4, samples[index], true);
          return window.noonFiniteCapture.base64(bytes);
        }, { first, count }), "PCM block");
        const bytes = Buffer.from(base64, "base64");
        assert.equal(bytes.length, count * 8);
        await pcmFile.writeFile(bytes);
        first += count;
      }
      await pcmFile.close(); pcmFile = null;
    }
    assert.deepEqual(errors, [], "browser capture errors");
    checkInterrupted();
    const manifest = {
      format: "noon-prepared-capture-v2", width: config.width, height: config.height,
      source: plan.source, source_end: plan.source_end, frame_options: plan.frame_options, audio: plan.audio, samples,
    };
    const json = JSON.stringify(manifest);
    assert.ok(Buffer.byteLength(json) <= 16 * 1024 * 1024);
    // The importable manifest appears only after exact sampling, media writes
    // and file closes. No partial capture can be mistaken for complete media.
    await writeFile(path.join(config.directory, "capture.json"), `${json}\n`, { flag: "wx" });
    checkInterrupted();
    captureComplete = true;
    const report = {
      source: config.sourcePath, source_sha256: sourceHash, capture: config.directory,
      renderer_backend: start.rendererBackend, transport: start.transportMode,
      authored_duration: start.authoredDuration, runtime_build_identity: start.runtimeBuildIdentity,
      sampling: plan.sampling, audio_frames: plan.audio_frames, observations: samples.length,
      capture_source: plan.source, preparation: start.preparation,
      browser_executable_sha256: browserExecutableSha256, local_pyodide_resources: localResources,
      browser_errors: errors, resource_cache: resourceCache.stats(),
    };
    await page.evaluate(async () => {
      const { execution, authoring } = window.noonFiniteCapture;
      await execution.cancelFiniteExport();
      execution.terminate(); authoring.terminate();
    });
    await browser.close(); browser = null;
    await server.close(); serverOpen = false;
    // A late browser error or cancellation cannot cross into native publication
    // merely because all pixel/PCM writes had already finished.
    assert.deepEqual(errors, [], "browser capture errors including teardown");
    checkInterrupted();
    if (config.output !== null) {
      const args = [config.directory, config.output, ...(config.overwrite ? ["--overwrite"] : [])];
      const executable = config.encoder ?? "cargo";
      const nativeArgs = config.encoder === null
        ? ["run", "--locked", "-p", "noon-export", "--example", "encode_prepared_capture", "--", ...args]
        : args;
      let stdout = "", stdoutBytes = 0;
      await new Promise((resolve, reject) => {
        const child = spawn(executable, nativeArgs, { cwd: repoRoot, stdio: ["ignore", "pipe", "inherit"] });
        encoderProcess = child;
        let encoderFailure = null;
        child.stdout.setEncoding("utf8");
        child.stdout.on("data", chunk => {
          stdoutBytes += Buffer.byteLength(chunk);
          if (stdoutBytes > 1024 * 1024) {
            encoderFailure ??= new Error("native exporter summary exceeds its budget");
            child.kill("SIGTERM");
          } else if (encoderFailure === null) stdout += chunk;
        });
        child.once("error", error => { encoderFailure ??= error; });
        // Wait for stdout as well as process exit. Retain the primary failure
        // while an oversized or cancelled child drains and actually retires.
        child.once("close", (code, signal) => {
          if (encoderProcess === child) encoderProcess = null;
          if (encoderFailure !== null) reject(encoderFailure);
          else if (interrupted) reject(new Error("Python capture cancelled"));
          else if (code === 0) resolve();
          else reject(new Error(`native export failed (${signal ?? code})`));
        });
      });
      checkInterrupted();
      report.native = JSON.parse(stdout);
    }
    return report;
  } catch (error) {
    failure = error;
    throw error;
  } finally {
    const closed = await Promise.allSettled([
      framesFile?.close(), pcmFile?.close(), browser?.close(), serverOpen ? server.close() : undefined,
    ]);
    if (directoryOwned && !captureComplete) {
      closed.push(...await Promise.allSettled([rm(config.directory, { recursive: true, force: true })]));
    }
    process.removeListener("SIGINT", interrupt);
    process.removeListener("SIGTERM", interrupt);
    const cleanupErrors = closed.filter(result => result.status === "rejected").map(result => result.reason);
    if (cleanupErrors.length) {
      if (failure === null) throw new AggregateError(cleanupErrors, "Python capture cleanup failed");
      for (const error of cleanupErrors) console.error("Python capture cleanup failed:", error);
    }
  }
}

try {
  const config = argumentsFor(process.argv.slice(2));
  if (config === null) console.log(USAGE);
  else console.log(JSON.stringify(await run(config)));
} catch (error) {
  console.error(error.stack ?? String(error));
  process.exitCode = 1;
}
