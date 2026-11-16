import { createHash } from "node:crypto";
import { readFile, realpath } from "node:fs/promises";
import path from "node:path";
import * as z from "zod/v4";

import { buildFrameFeedback, feedbackInputSchema, readCapturedFrame } from "./frame-feedback.mjs";
import {
  jsonBytes, jsonFile, MAX_MANIFEST_BYTES, previewFrame, previewFrames, sourceFile, writeBundle,
} from "./feedback-files.mjs";

const hash = z.string().regex(/^[a-f0-9]{64}$/);
const noteSchema = z.strictObject({
  kind: z.literal("noon-frame-feedback-note"), schemaVersion: z.literal(1),
  sourceSha256: hash, sampleIndex: z.number().int().min(0).max(31), frameIdentity: hash,
  feedback: feedbackInputSchema,
});

// The canonical parsed frame has stable field order and includes all observed
// source/build/run/time/image metadata. Lease retention is intentionally absent:
// this is persisted review context, never a capability into a live session.
function frameIdentity(frame) {
  return createHash("sha256").update("noon-frame-review-v1\0").update(JSON.stringify(frame)).digest("hex");
}

function embeddedJson(value) {
  return jsonBytes(value).toString("utf8").replace(/[<>&\u2028\u2029]/g,
    (character) => `\\u${character.charCodeAt(0).toString(16).padStart(4, "0")}`);
}

/** Create a local static view of already verified captures; execute no source. */
export async function createFeedbackReview({ previewPath, sourcePath, outputDir }) {
  const source = await sourceFile(sourcePath);
  const captured = await previewFrames(previewPath, source.sha256);
  const files = { "source.txt": source.bytes };
  const frames = captured.frames.map((retained, sampleIndex) => {
    const filename = `frame-${String(sampleIndex).padStart(3, "0")}.png`;
    files[filename] = retained.png;
    return { sampleIndex, identity: frameIdentity(retained.frame), filename, frame: retained.frame };
  });
  const data = {
    kind: "noon-frame-review", schemaVersion: 1,
    source: { name: path.basename(source.resolved), sha256: source.sha256 }, frames,
  };
  // Keep the original observations for canonical import. Only host file paths
  // change when moving a capture into this portable review folder.
  const manifest = {
    ...captured.manifest,
    source: { path: "source.txt", sha256: source.sha256 },
    samples: captured.manifest.samples.map((sample, index) => ({ ...sample, filename: frames[index].filename })),
  };
  files["manifest.json"] = jsonBytes(manifest, MAX_MANIFEST_BYTES);
  const template = await readFile(new URL("../review/review.html", import.meta.url), "utf8");
  if (template.split("__NOON_REVIEW_DATA__").length !== 2) throw new Error("review template must have one data placeholder");
  files["index.html"] = template.replace("__NOON_REVIEW_DATA__", () => embeddedJson(data));
  for (const filename of ["review.css", "review.js"]) {
    files[filename] = await readFile(new URL(`../review/${filename}`, import.meta.url));
  }
  await writeBundle(outputDir, files);
  return Object.freeze({ outputDir, indexPath: path.join(outputDir, "index.html"), frames: frames.length });
}

/** Finalize a browser note through the same builder used by CLI/MCP annotation. */
export async function importFeedbackReview({ reviewDir, notePath, outputDir }) {
  const root = await realpath(reviewDir);
  const source = await sourceFile(path.join(root, "source.txt"), root);
  const note = noteSchema.parse((await jsonFile(notePath)).value);
  if (note.sourceSha256 !== source.sha256) throw new Error("review note does not match the preserved source");
  const retained = await previewFrame(path.join(root, "manifest.json"), note.sampleIndex, source.sha256, root);
  const frame = readCapturedFrame(retained);
  if (note.frameIdentity !== frameIdentity(frame)) {
    throw new Error("review note does not match the selected captured frame identity");
  }
  const feedback = buildFrameFeedback(retained, note.feedback);
  await writeBundle(outputDir, {
    "feedback.json": jsonBytes(feedback), "frame.png": retained.png, "source.txt": source.bytes,
  });
  return Object.freeze({ outputDir, feedbackPath: path.join(outputDir, "feedback.json"), feedback });
}
