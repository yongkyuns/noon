import { createHash } from "node:crypto";
import path from "node:path";
import { readFrameFeedback } from "./frame-feedback.mjs";
import { jsonBytes, jsonFile, MAX_PNG_BYTES, MAX_SOURCE_BYTES, writeBundle } from "./feedback-files.mjs";

// JSON includes base64 PNG and possibly escaped source text. This is a file
// exchange boundary, not an engine message or an alternate scene representation.
const MAX_LIVE_EXPORT_BYTES = 18 * 1024 * 1024;
function exactKeys(value, keys, label) {
  if (!value || typeof value !== "object" || Array.isArray(value) ||
      Object.keys(value).length !== keys.length || keys.some(key => !Object.hasOwn(value, key))) {
    throw new Error(`Invalid ${label} fields.`);
  }
}

export async function importLiveFeedback({ inputPath, outputDir }) {
  const { value } = await jsonFile(inputPath, MAX_LIVE_EXPORT_BYTES);
  exactKeys(value, ["kind", "schemaVersion", "source", "pngBase64", "presentationReceipt", "feedback"], "live feedback export");
  if (value.kind !== "noon-live-feedback-export" || value.schemaVersion !== 1) {
    throw new Error("Unsupported live feedback export.");
  }
  if (typeof value.source !== "string" || !value.source.trim() || value.source.includes("\0") ||
      Buffer.byteLength(value.source, "utf8") > MAX_SOURCE_BYTES) {
    throw new Error("Live feedback requires a bounded source snapshot.");
  }
  const source = Buffer.from(value.source, "utf8");
  if (source.toString("utf8") !== value.source) throw new Error("Source contains invalid Unicode.");
  if (typeof value.pngBase64 !== "string" || value.pngBase64.length === 0 ||
      value.pngBase64.length > 4 * Math.ceil(MAX_PNG_BYTES / 3) || value.pngBase64.length % 4 !== 0) {
    throw new Error("Live feedback PNG exceeds its encoded byte limit.");
  }
  const png = Buffer.from(value.pngBase64, "base64");
  if (png.length === 0 || png.length > MAX_PNG_BYTES || png.toString("base64") !== value.pngBase64) {
    throw new Error("Live feedback requires canonical base64 PNG bytes.");
  }
  // Reuse the existing record, PNG framing, dimensions and digest validation.
  const feedback = readFrameFeedback(value.feedback, png);
  const sourceHash = createHash("sha256").update(source).digest("hex");
  if (sourceHash !== feedback.frame.provenance.sourceSha256) {
    throw new Error("Live feedback source does not match the captured Run.");
  }
  const receipt = value.presentationReceipt;
  exactKeys(receipt, ["session", "sequence", "time"], "presentation receipt");
  const run = /^playground:(0|[1-9][0-9]*):(0|[1-9][0-9]*)$/.exec(feedback.frame.provenance.sessionId);
  if (!Number.isSafeInteger(receipt.session) || receipt.session < 0 ||
      !Number.isSafeInteger(receipt.sequence) || receipt.sequence < 0 ||
      !Number.isFinite(receipt.time) || receipt.time !== feedback.frame.provenance.publishedTime ||
      receipt.time !== feedback.frame.provenance.requestedTime ||
      !run || !Number.isSafeInteger(Number(run[1])) || Number(run[2]) !== receipt.session) {
    throw new Error("Live feedback receipt does not match its captured publication.");
  }
  // These checks establish internal consistency, not authenticity of a supplied
  // file. Import never executes the source or accepts a proposed correction.
  await writeBundle(outputDir, {
    "feedback.json": jsonBytes(feedback), "frame.png": png, "source.txt": source,
    "capture.json": jsonBytes({ kind: value.kind, schemaVersion: 1, presentationReceipt: receipt }),
  });
  return Object.freeze({ outputDir, feedbackPath: path.join(outputDir, "feedback.json"), feedback });
}
