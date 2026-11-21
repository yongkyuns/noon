import { resolveSourceFeedback, sha256 } from "./playground-feedback-capture.js";

const MAX_FILE_BYTES = 23 * 1024 * 1024;
const MAX_PNG_BYTES = 8 * 1024 * 1024;
const MAX_PIXELS = 16_777_216;
const PNG_HEADER = [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82];
const isHash = value => typeof value === "string" && /^[a-f0-9]{64}$/.test(value);

async function verifiedImage(frame, base64) {
  const time = frame?.provenance?.publishedTime;
  if (frame?.mimeType !== "image/png" || !Number.isInteger(frame.width) || !Number.isInteger(frame.height) ||
      frame.width < 1 || frame.height < 1 || frame.width > 8192 || frame.height > 8192 ||
      frame.width * frame.height > MAX_PIXELS || !Number.isFinite(time) || time < 0 || time > 600 ||
      !isHash(frame.sha256) || typeof base64 !== "string" || base64.length > 4 * Math.ceil(MAX_PNG_BYTES / 3)) {
    throw new Error("The comparison has invalid picture metadata.");
  }
  const binary = atob(base64);
  const png = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) png[i] = binary.charCodeAt(i);
  const view = new DataView(png.buffer);
  if (png.length < 33 || png.length > MAX_PNG_BYTES || png.length !== frame.byteLength ||
      !PNG_HEADER.every((value, i) => png[i] === value) ||
      view.getUint32(16) !== frame.width || view.getUint32(20) !== frame.height ||
      await sha256(png) !== frame.sha256) {
    throw new Error("A picture does not match the comparison evidence.");
  }
  return { blob: new Blob([png], { type: "image/png" }), time, width: frame.width, height: frame.height };
}

export async function readViewerComparison(file, source) {
  if (file.size === 0 || file.size > MAX_FILE_BYTES) {
    throw new Error("Choose a viewer comparison file of at most 23 MiB.");
  }
  const { comparison, beforePngBase64, afterPngBase64 } = JSON.parse(await file.text()) ?? {};
  const before = comparison?.before;
  const after = comparison?.after;
  const original = before?.frame?.provenance;
  if (comparison?.kind !== "noon-frame-feedback-comparison" || comparison.schemaVersion !== 1 ||
      comparison.status !== "pending_review" || before?.kind !== "noon-frame-feedback" ||
      before.schemaVersion !== 1 || before.status !== "pending_review" ||
      typeof original?.sessionId !== "string" || !original.sessionId.startsWith("playground:") ||
      !isHash(original.sourceSha256) || !isHash(after?.provenance?.sourceSha256) ||
      before.feedback?.interval?.start !== original.publishedTime ||
      before.feedback?.interval?.end !== original.publishedTime) {
    throw new Error("Choose a Playground comparison created by noon-feedback compare --viewer.");
  }
  const encodedSource = new TextEncoder().encode(source);
  if (encodedSource.length > 1_000_000 || await sha256(encodedSource) !== after.provenance.sourceSha256) {
    throw new Error("The correction was rendered from a different source. Load the reviewed Python draft or render a new comparison.");
  }
  const resolvedSource = resolveSourceFeedback(source, before);
  const pictures = await Promise.all([verifiedImage(before.frame, beforePngBase64), verifiedImage(after, afterPngBase64)]);
  return { resolvedSource, observation: before.feedback.observation, pictures };
}
