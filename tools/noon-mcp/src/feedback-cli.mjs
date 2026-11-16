import { constants } from "node:fs";
import { mkdir, open, realpath, rm, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";

import {
  buildFeedbackComparison, buildFrameFeedback, checkFeedbackSource, readFrameFeedback,
} from "./frame-feedback.mjs";

const MAX_JSON_BYTES = 128 * 1024;
const MAX_MANIFEST_BYTES = 1024 * 1024;
const MAX_SOURCE_BYTES = 1_000_000;
const MAX_PNG_BYTES = 8 * 1024 * 1024;
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const utf8 = (bytes) => new TextDecoder("utf-8", { fatal: true }).decode(bytes);

export function parseFeedbackCliArgs(argv) {
  if (!Array.isArray(argv) || argv.some((value) => typeof value !== "string")) {
    throw new TypeError("feedback CLI arguments must be strings");
  }
  if (argv.includes("--help") || argv.includes("-h")) return { help: true };
  const command = argv[0];
  const flags = {
    annotate: ["preview", "sample", "source", "input", "output"],
    check: ["feedback", "source"],
    compare: ["feedback", "candidate", "sample", "source", "output"],
  }[command];
  if (!flags) throw new TypeError("expected feedback command: annotate, check, or compare");
  const args = { command, sample: 0 };
  for (let index = 1; index < argv.length; index += 2) {
    const name = argv[index].replace(/^--/, "");
    if (argv[index] !== `--${name}` || !flags.includes(name)) {
      throw new TypeError(`unknown ${command} argument: ${argv[index]}`);
    }
    if (Object.hasOwn(args, name) && name !== "sample") throw new TypeError(`duplicate --${name}`);
    if (name === "sample" && args.sampleSupplied) throw new TypeError("duplicate --sample");
    const value = argv[index + 1];
    if (typeof value !== "string" || value.trim() === "" || value.includes("\0")) {
      throw new TypeError(`--${name} requires a non-empty value without NUL`);
    }
    if (name === "sample") {
      if (!/^(0|[1-9][0-9]*)$/.test(value) || Number(value) > 31) {
        throw new RangeError("--sample must be an integer from 0 to 31");
      }
      args.sample = Number(value);
      args.sampleSupplied = true;
    } else args[name] = value;
  }
  for (const name of flags.filter((value) => value !== "sample")) {
    if (!Object.hasOwn(args, name)) throw new TypeError(`--${name} is required`);
  }
  delete args.sampleSupplied;
  return Object.freeze(args);
}

async function boundedFile(filename, maxBytes, parent = null) {
  const resolved = await realpath(filename);
  if (parent !== null && path.dirname(resolved) !== parent) {
    throw new Error("feedback input escapes its artifact directory");
  }
  const file = await open(resolved, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const metadata = await file.stat();
    if (!metadata.isFile() || metadata.size > maxBytes) {
      throw new Error("feedback input must be a regular file within the byte limit");
    }
    // Bound the read itself too: a concurrently growing file cannot allocate
    // beyond the budget between stat and read.
    const bytes = Buffer.alloc(maxBytes + 1);
    let size = 0;
    while (size < bytes.length) {
      const { bytesRead } = await file.read(bytes, size, bytes.length - size, null);
      if (bytesRead === 0) break;
      size += bytesRead;
    }
    if (size > maxBytes) throw new Error("feedback input exceeds byte limit");
    return { resolved, bytes: bytes.subarray(0, size) };
  } finally {
    await file.close();
  }
}

async function jsonFile(filename, maxBytes = MAX_JSON_BYTES) {
  const file = await boundedFile(filename, maxBytes);
  return { ...file, value: JSON.parse(utf8(file.bytes)) };
}

async function sourceFile(filename, parent = null) {
  const file = await boundedFile(filename, MAX_SOURCE_BYTES, parent);
  const text = utf8(file.bytes);
  if (text.trim() === "" || text.includes("\0")) throw new Error("source must be non-empty UTF-8 text without NUL");
  return { ...file, sha256: hash(file.bytes) };
}

async function previewFrame(filename, sampleIndex, sourceSha256) {
  const { resolved, value: manifest } = await jsonFile(filename, MAX_MANIFEST_BYTES);
  if (manifest?.schema !== 2 || !Array.isArray(manifest.samples) ||
      manifest.samples.length === 0 || manifest.samples.length > 32 ||
      !Number.isInteger(sampleIndex) || sampleIndex < 0 || sampleIndex >= manifest.samples.length) {
    throw new Error("expected a schema-2 preview manifest and an existing sample index");
  }
  const sample = manifest.samples[sampleIndex];
  // A manifest never chooses a source path to read. The caller supplies it, and
  // only a flat PNG filename relative to this manifest can supply image bytes.
  if (typeof sample?.filename !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._-]*\.png$/.test(sample.filename)) {
    throw new Error("preview sample requires a confined PNG filename");
  }
  const provenance = sample.artifact?.provenance;
  if (manifest.source?.sha256 !== sourceSha256 || provenance?.sourceSha256 !== sourceSha256) {
    throw new Error("preview frame does not match the supplied source; its association is stale");
  }
  if (sample.snapshot?.frame?.requestedTime !== provenance.requestedTime ||
      sample.snapshot?.frame?.publishedTime !== provenance.publishedTime ||
      sample.snapshot?.frame?.rendererBackend !== provenance.backend) {
    throw new Error("preview snapshot does not match the captured frame provenance");
  }
  const build = sample.snapshot?.buildIdentity;
  if (!build || !provenance.build ||
      (build.sourceRevision ?? null) !== provenance.build.engineRevision ||
      build.files?.wasm?.sha256 !== provenance.build.wasmSha256 ||
      build.files?.worker?.sha256 !== provenance.build.workerSha256 ||
      build.buildId !== provenance.build.buildId) {
    throw new Error("preview snapshot build identity does not match the captured frame provenance");
  }
  const image = sample.snapshot?.image;
  if (!image || ["mimeType", "width", "height", "sha256"].some((key) => image[key] !== sample.artifact[key]) ||
      image.encodedBytes !== sample.artifact.byteLength) {
    throw new Error("preview snapshot image identity does not match the captured frame provenance");
  }
  const parent = path.dirname(resolved);
  const { bytes: png } = await boundedFile(path.join(parent, sample.filename), MAX_PNG_BYTES, parent);
  return { descriptor: sample.artifact, png };
}

async function feedbackBundle(filename) {
  const { resolved, value } = await jsonFile(filename);
  const parent = path.dirname(resolved);
  const { bytes: png } = await boundedFile(path.join(parent, "frame.png"), MAX_PNG_BYTES, parent);
  const record = readFrameFeedback(value, png);
  const source = await sourceFile(path.join(parent, "source.txt"), parent);
  if (source.sha256 !== record.frame.provenance.sourceSha256) {
    throw new Error("feedback source snapshot does not match its recorded identity");
  }
  return { record, png, source };
}

async function writeBundle(outputDir, files) {
  // Never reuse an occupied output directory or overwrite accepted evidence.
  // All validation is finished before this directory is admitted.
  await mkdir(path.dirname(outputDir), { recursive: true });
  await mkdir(outputDir);
  try {
    for (const [filename, bytes] of Object.entries(files)) {
      await writeFile(path.join(outputDir, filename), bytes, { flag: "wx" });
    }
  } catch (error) {
    await rm(outputDir, { recursive: true, force: true }).catch(() => {});
    throw error;
  }
}

function jsonBytes(value) {
  const bytes = Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
  if (bytes.length > MAX_JSON_BYTES) throw new Error("feedback output exceeds byte limit");
  return bytes;
}

/** Offline evidence packaging only; never starts a renderer or changes a scene. */
export async function runFeedbackCli({ argv, cwd = process.cwd() }) {
  const args = parseFeedbackCliArgs(argv);
  if (args.help) return Object.freeze({ help: true });
  const source = await sourceFile(path.resolve(cwd, args.source));
  if (args.command === "annotate") {
    const input = await jsonFile(path.resolve(cwd, args.input));
    const retained = await previewFrame(path.resolve(cwd, args.preview), args.sample, source.sha256);
    const feedback = buildFrameFeedback(retained, input.value);
    const outputDir = path.resolve(cwd, args.output);
    await writeBundle(outputDir, {
      "feedback.json": jsonBytes(feedback), "frame.png": retained.png, "source.txt": source.bytes,
    });
    return Object.freeze({ outputDir, feedbackPath: path.join(outputDir, "feedback.json"), feedback });
  }

  const original = await feedbackBundle(path.resolve(cwd, args.feedback));
  if (args.command === "check") return checkFeedbackSource(original.record, source.sha256);

  const candidate = await previewFrame(path.resolve(cwd, args.candidate), args.sample, source.sha256);
  const comparison = buildFeedbackComparison(original.record, candidate, source.sha256);
  const outputDir = path.resolve(cwd, args.output);
  await writeBundle(outputDir, {
    "comparison.json": jsonBytes(comparison),
    "before.png": original.png, "after.png": candidate.png,
    "before-source.txt": original.source.bytes, "after-source.txt": source.bytes,
  });
  return Object.freeze({ outputDir, comparisonPath: path.join(outputDir, "comparison.json"), comparison });
}
