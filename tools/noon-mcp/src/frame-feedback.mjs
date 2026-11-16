import { Buffer } from "node:buffer";
import * as z from "zod/v4";

import { inspectCapturedPng } from "../../../scripts/agent-preview-artifacts.mjs";

export const MAX_FEEDBACK_TEXT_BYTES = 4000;
const time = z.number().finite().nonnegative().max(600);
const hash = z.string().regex(/^[a-f0-9]{64}$/);
const boundedText = (maxBytes) => z.string().refine((value) =>
  value.trim().length > 0 && !value.includes("\0") && Buffer.byteLength(value, "utf8") <= maxBytes,
"text must be non-empty, without NUL, and within its UTF-8 byte limit");
const reviewerText = boundedText(MAX_FEEDBACK_TEXT_BYTES);
const interval = z.strictObject({ start: time, end: time }).refine(
  (value) => value.end >= value.start, "interval end must not precede start");
const region = z.strictObject({
  x: z.number().finite().min(0).max(1),
  y: z.number().finite().min(0).max(1),
  width: z.number().finite().positive().max(1),
  height: z.number().finite().positive().max(1),
}).refine((value) => value.x + value.width <= 1 && value.y + value.height <= 1,
  "region must fit inside the normalized image rectangle");

// Review metadata at a persistence/MCP boundary. These records neither resolve
// semantic targets nor authorize mutations, schedule scenes, or retime cues.
export const feedbackInputSchema = z.strictObject({
  interval,
  region: region.optional(),
  observation: reviewerText,
  desiredChange: reviewerText,
  preserve: reviewerText.optional(),
});
const feedbackBodySchema = feedbackInputSchema.extend({
  region: region.nullable(), preserve: reviewerText.nullable(),
});
const revision = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER).nullable();
const frameSchema = z.strictObject({
  id: boundedText(256),
  mimeType: z.literal("image/png"),
  width: z.number().int().positive().max(8192),
  height: z.number().int().positive().max(8192),
  byteLength: z.number().int().positive().max(8 * 1024 * 1024),
  sha256: hash,
  provenance: z.strictObject({
    sessionId: boundedText(256), sourceSha256: hash,
    build: z.strictObject({
      engineRevision: z.string().regex(/^[a-f0-9]{40}$/).nullable(),
      wasmSha256: hash.nullable(), workerSha256: hash.nullable(),
      buildId: boundedText(256).nullable(),
    }),
    requestedTime: time, publishedTime: time, backend: boundedText(64),
    sceneRevision: revision, frameRevision: revision,
  }),
});
const descriptorSchema = frameSchema.extend({ retentionMs: z.number().int().positive() });
const feedbackRecordSchema = z.strictObject({
  kind: z.literal("noon-frame-feedback"), schemaVersion: z.literal(1),
  status: z.literal("pending_review"), feedback: feedbackBodySchema, frame: frameSchema,
}).refine((record) => record.feedback.interval.start <= record.frame.provenance.publishedTime &&
  record.feedback.interval.end >= record.frame.provenance.publishedTime,
"feedback interval must contain the published frame time");

function freeze(value) {
  if (value && typeof value === "object") {
    for (const item of Object.values(value)) freeze(item);
    Object.freeze(value);
  }
  return value;
}

function verifyImage(frame, png) {
  const actual = inspectCapturedPng(png);
  if (["sha256", "byteLength", "width", "height"].some((key) => frame[key] !== actual[key])) {
    throw new Error("feedback PNG does not match its recorded identity and dimensions");
  }
}

export function readCapturedFrame(retained) {
  const { retentionMs: _retentionMs, ...frame } = descriptorSchema.parse(retained?.descriptor);
  verifyImage(frame, retained?.png);
  return freeze(frame);
}

export function buildFrameFeedback(retained, input) {
  const note = feedbackInputSchema.parse(input);
  return freeze(feedbackRecordSchema.parse({
    kind: "noon-frame-feedback", schemaVersion: 1, status: "pending_review",
    feedback: { ...note, region: note.region ?? null, preserve: note.preserve ?? null },
    frame: readCapturedFrame(retained),
  }));
}

export function readFrameFeedback(record, png) {
  const parsed = feedbackRecordSchema.parse(record);
  verifyImage(parsed.frame, png);
  return freeze(parsed);
}

export function checkFeedbackSource(record, currentSourceSha256) {
  const parsed = feedbackRecordSchema.parse(record);
  const current = hash.parse(currentSourceSha256);
  const observed = parsed.frame.provenance.sourceSha256;
  return freeze({
    status: "pending_review", sourceStatus: observed === current ? "current" : "stale",
    observedSourceSha256: observed, currentSourceSha256: current,
  });
}

export function buildFeedbackComparison(record, candidate, currentSourceSha256) {
  const before = feedbackRecordSchema.parse(record);
  const after = readCapturedFrame(candidate);
  const association = checkFeedbackSource(before, currentSourceSha256);
  if (after.provenance.sourceSha256 !== association.currentSourceSha256) {
    throw new Error("candidate frame does not match the supplied current source");
  }
  return freeze({
    kind: "noon-frame-feedback-comparison", schemaVersion: 1, status: "pending_review",
    sourceAssociation: association,
    // A selected candidate frame is evidence, not proof that a time interval,
    // cue, semantic target, speech, or the explanation itself was corrected.
    intervalAssociation: "unverified", before, after,
  });
}
