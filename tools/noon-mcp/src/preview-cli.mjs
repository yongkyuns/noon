import { createHash } from "node:crypto";
import { mkdir, readFile, realpath, stat, writeFile } from "node:fs/promises";
import path from "node:path";

const MAX_SOURCE_BYTES = 1_000_000;
// The composed service retains the initial frame too, so 31 requested samples
// fit its default 32-frame scope without advancing into an unpublishable frame.
export const MAX_PREVIEW_CLI_SAMPLES = 31;
const MAX_TIME_SECONDS = 600;

function finiteTime(name, text, { positive = false } = {}) {
  if (typeof text !== "string" || text.trim() === "") throw new TypeError(`${name} requires a value`);
  const value = Number(text);
  if (!Number.isFinite(value) || value < 0 || value > MAX_TIME_SECONDS || (positive && value === 0)) {
    throw new RangeError(`${name} must be ${positive ? "positive" : "non-negative"}, finite, and <= ${MAX_TIME_SECONDS}`);
  }
  return value;
}

export function parsePreviewCliArgs(argv) {
  if (!Array.isArray(argv) || argv.some((value) => typeof value !== "string")) {
    throw new TypeError("preview CLI arguments must be strings");
  }
  let sourcePath = null;
  let outputDir = "noon-preview-output";
  let loopDurationSeconds = 4;
  const times = [];
  let help = false;
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--help" || arg === "-h") { help = true; continue; }
    const next = () => {
      index += 1;
      if (index >= argv.length) throw new TypeError(`${arg} requires a value`);
      return argv[index];
    };
    if (arg === "--source") sourcePath = next();
    else if (arg === "--output") outputDir = next();
    else if (arg === "--loop-duration") loopDurationSeconds = finiteTime("--loop-duration", next(), { positive: true });
    else if (arg === "--time") times.push(finiteTime("--time", next()));
    else throw new TypeError(`unknown preview CLI argument: ${arg}`);
  }
  if (!help && (typeof sourcePath !== "string" || sourcePath.trim() === "")) {
    throw new TypeError("--source is required");
  }
  if (times.length > MAX_PREVIEW_CLI_SAMPLES) {
    throw new RangeError(`at most ${MAX_PREVIEW_CLI_SAMPLES} --time values are allowed`);
  }
  for (let index = 1; index < times.length; index += 1) {
    if (times[index] < times[index - 1]) throw new RangeError("--time values must be nondecreasing");
  }
  if (typeof outputDir !== "string" || outputDir.trim() === "" || outputDir.includes("\0")) {
    throw new TypeError("--output must be a non-empty path without NUL");
  }
  return Object.freeze({ sourcePath, outputDir, loopDurationSeconds, times: Object.freeze(times), help });
}

async function sourceFile(filename, cwd) {
  const resolved = await realpath(path.resolve(cwd, filename));
  const metadata = await stat(resolved);
  if (!metadata.isFile() || metadata.size > MAX_SOURCE_BYTES) {
    throw new Error("preview source must be a regular file within the byte limit");
  }
  const bytes = await readFile(resolved);
  if (bytes.length > MAX_SOURCE_BYTES) throw new Error("preview source exceeds byte limit");
  const source = bytes.toString("utf8");
  if (source.trim() === "" || source.includes("\0")) throw new Error("preview source must be non-empty UTF-8 text without NUL");
  return { resolved, source, sha256: createHash("sha256").update(bytes).digest("hex") };
}

function requireService(service) {
  const methods = ["openScope", "open", "sampleFrames", "getArtifact", "close", "closeScope", "dispose"];
  if (!service || methods.some((name) => typeof service[name] !== "function")) {
    throw new TypeError("preview CLI service factory returned an invalid service");
  }
  return service;
}

function retainedFrame(service, scope, sessionId, descriptor, snapshot, expectedSourceSha256) {
  if (!descriptor || typeof descriptor !== "object" || typeof descriptor.id !== "string") {
    throw new Error("preview service returned no artifact descriptor");
  }
  const retained = service.getArtifact(scope, sessionId, descriptor.id);
  if (!retained || !Buffer.isBuffer(retained.png) || !retained.descriptor || retained.descriptor.id !== descriptor.id) {
    throw new Error("preview service returned no retained PNG artifact");
  }
  const actualSha256 = createHash("sha256").update(retained.png).digest("hex");
  const provenance = retained.descriptor.provenance;
  if (retained.descriptor.mimeType !== "image/png" ||
      retained.descriptor.sha256 !== actualSha256 ||
      retained.descriptor.byteLength !== retained.png.length ||
      provenance?.sessionId !== sessionId ||
      provenance?.sourceSha256 !== expectedSourceSha256 ||
      provenance?.requestedTime !== snapshot?.frame?.requestedTime ||
      provenance?.backend !== snapshot?.frame?.rendererBackend) {
    throw new Error("retained preview artifact does not match the coherent service observation");
  }
  return retained;
}

function sampleRecord(snapshot, retained, filename) {
  return Object.freeze({
    filename,
    snapshot,
    artifact: retained.descriptor,
  });
}

function attachCleanupError(primaryError, error) {
  const message = String(error?.message ?? error);
  if (primaryError && typeof primaryError === "object") {
    primaryError.cleanupError = primaryError.cleanupError ? `${primaryError.cleanupError}; ${message}` : message;
  }
}

export async function runPreviewCli({ argv, cwd = process.cwd(), serviceFactory }) {
  const args = parsePreviewCliArgs(argv);
  if (args.help) return Object.freeze({ help: true });
  if (typeof serviceFactory !== "function") throw new TypeError("preview CLI requires a service factory");
  const source = await sourceFile(args.sourcePath, cwd);
  const outputDir = path.resolve(cwd, args.outputDir);
  await mkdir(outputDir, { recursive: true });

  const service = requireService(serviceFactory());
  let scope = null;
  let sessionId = null;
  let primaryError = null;
  let cleanup = null;
  const samples = [];

  try {
    scope = service.openScope();
    const opened = await service.open(scope, source.source, { loopDurationSeconds: args.loopDurationSeconds });
    sessionId = opened.sessionId;
    const initial = retainedFrame(service, scope, sessionId, opened.artifact, opened.snapshot, source.sha256);
    const initialName = "frame-000.png";
    await writeFile(path.join(outputDir, initialName), initial.png, { flag: "wx" });
    samples.push(sampleRecord(opened.snapshot, initial, initialName));

    const requestedTimes = args.times.filter((time) => time !== 0);
    if (requestedTimes.length > 0) {
      const frames = await service.sampleFrames(scope, sessionId, requestedTimes);
      let ordinal = 1;
      for (const frame of frames) {
        const retained = retainedFrame(service, scope, sessionId, frame.artifact, frame.snapshot, source.sha256);
        const filename = `frame-${String(ordinal).padStart(3, "0")}.png`;
        ordinal += 1;
        await writeFile(path.join(outputDir, filename), retained.png, { flag: "wx" });
        samples.push(sampleRecord(frame.snapshot, retained, filename));
      }
    }
  } catch (error) {
    primaryError = error;
  } finally {
    if (scope !== null && sessionId !== null) {
      try { cleanup = await service.close(scope, sessionId, primaryError ? "preview CLI failed" : "preview CLI complete"); }
      catch (error) {
        if (!primaryError) primaryError = error;
        else attachCleanupError(primaryError, error);
      }
    }
    if (scope !== null) {
      try { await service.closeScope(scope, primaryError ? "preview CLI scope failed" : "preview CLI scope complete"); }
      catch (error) {
        if (!primaryError) primaryError = error;
        else attachCleanupError(primaryError, error);
      }
    }
    try { await service.dispose(primaryError ? "preview CLI service failed" : "preview CLI service complete"); }
    catch (error) {
      if (!primaryError) primaryError = error;
      else attachCleanupError(primaryError, error);
    }
  }

  if (primaryError) throw primaryError;
  const manifest = {
    schema: 2,
    source: { path: source.resolved, sha256: source.sha256 },
    loopDurationSeconds: args.loopDurationSeconds,
    samples,
    cleanup,
  };
  const manifestPath = path.join(outputDir, "manifest.json");
  await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, { flag: "wx" });
  return Object.freeze({ outputDir, manifestPath, manifest: Object.freeze(manifest) });
}
