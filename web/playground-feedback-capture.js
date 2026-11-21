// Browser-host review artifacts only. Playback, scene time and publication
// identity remain owned by the existing Rust execution endpoint.
const MAX_SOURCE_BYTES = 1_000_000;
const MAX_PNG_BYTES = 8 * 1024 * 1024;
const MAX_PIXELS = 16_777_216;
const encoder = new TextEncoder();

function requireCapture(condition, message) {
  if (!condition) throw new Error(message);
}

export function feedbackImageBounds(box, width, height) {
  if (![box?.left, box?.top, box?.width, box?.height, width, height].every(Number.isFinite) ||
      box.width <= 0 || box.height <= 0 || width <= 0 || height <= 0) return null;
  const scale = Math.min(box.width / width, box.height / height);
  const imageWidth = width * scale;
  const imageHeight = height * scale;
  return {
    left: box.left + (box.width - imageWidth) / 2,
    top: box.top + (box.height - imageHeight) / 2,
    width: imageWidth,
    height: imageHeight,
  };
}

export function feedbackPoint(clientX, clientY, box, width, height, clamp = false) {
  const bounds = feedbackImageBounds(box, width, height);
  if (!bounds || !Number.isFinite(clientX) || !Number.isFinite(clientY)) return null;
  const x = (clientX - bounds.left) / bounds.width;
  const y = (clientY - bounds.top) / bounds.height;
  if (clamp) return { x: Math.max(0, Math.min(1, x)), y: Math.max(0, Math.min(1, y)) };
  return x >= 0 && x <= 1 && y >= 0 && y <= 1 ? { x, y } : null;
}

export function feedbackRegion(start, end) {
  if (start.x === end.x || start.y === end.y) return null;
  return {
    x: Math.min(start.x, end.x), y: Math.min(start.y, end.y),
    width: Math.abs(start.x - end.x), height: Math.abs(start.y - end.y),
  };
}

export function assertPausedPublication(state, expected = null) {
  const receipt = state?.presentationReceipt;
  requireCapture(state?.playing === false && state.preparedPlayback === true,
    "Comment capture requires paused prepared playback.");
  requireCapture(Number.isFinite(state.time) && state.time >= 0 && state.time <= 600,
    "This feedback format supports captured times from 0 to 600 seconds.");
  requireCapture(Number.isSafeInteger(receipt?.session) && receipt.session >= 0 &&
    Number.isSafeInteger(receipt?.sequence) && receipt.sequence >= 0 && receipt.time === state.time,
  "The paused picture has no matching presentation receipt. Try again after it is displayed.");
  if (expected !== null) {
    requireCapture(receipt.session === expected.session && receipt.sequence === expected.sequence &&
      receipt.time === expected.time, "The displayed frame changed during comment capture. Try again.");
  }
  return Object.freeze({ session: receipt.session, sequence: receipt.sequence, time: receipt.time });
}

export async function sha256(bytes) {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), value => value.toString(16).padStart(2, "0")).join("");
}

function observedBuild(build) {
  const hash = (value, length) => typeof value === "string" &&
    new RegExp(`^[a-f0-9]{${length}}$`).test(value) ? value : null;
  return Object.freeze({
    engineRevision: hash(build?.sourceRevision, 40),
    wasmSha256: hash(build?.files?.wasm?.sha256, 64),
    workerSha256: hash(build?.files?.worker?.sha256, 64),
    buildId: typeof build?.buildId === "string" && build.buildId.trim() &&
      !build.buildId.includes("\0") && encoder.encode(build.buildId).length <= 256 ? build.buildId : null,
  });
}

// context is captured by the viewer from its actual run, not from editor text.
// copyImage must synchronously copy the displayed canvas before returning its
// asynchronous PNG encoding. Check publications on both sides of that copy.
export async function capturePausedFeedback(context, {
  copyImage, afterPaint, signal, onPaused = () => {},
} = {}) {
  requireCapture(typeof copyImage === "function" && typeof afterPaint === "function",
    "Comment capture requires the viewer's image-copy and presentation adapters.");
  requireCapture(typeof context?.source === "string" && context.source.trim() &&
    !context.source.includes("\0") && encoder.encode(context.source).length <= MAX_SOURCE_BYTES &&
    new TextDecoder().decode(encoder.encode(context.source)) === context.source,
  "The displayed Run has no bounded source snapshot.");
  requireCapture(Number.isSafeInteger(context.runGeneration) && context.runGeneration >= 0,
    "The displayed Run has no valid generation.");
  const check = () => {
    if (signal?.aborted || !context.isCurrent()) {
      throw new DOMException("Comment capture was cancelled or its Run was replaced.", "AbortError");
    }
  };
  check();
  const source = context.source;
  const paused = await context.player.pause();
  check();
  onPaused(paused);
  requireCapture(paused?.playing === false && paused.preparedPlayback === true &&
    Number.isFinite(paused.time) && paused.time >= 0 && paused.time <= 600,
  "Comment capture is available for paused prepared playback. Finish preparing this scene first.");
  // Re-present this exact authored time through the existing player. This is
  // not a new clock, an extrapolated frame time, or a source replay.
  const presented = await context.player.seek(paused.time);
  check();
  const receipt = assertPausedPublication(presented);
  requireCapture(receipt.time === paused.time, "The player did not present the requested paused time.");
  onPaused(presented);
  const build = await context.buildIdentity();
  check();
  await afterPaint(signal);
  check();
  assertPausedPublication(await context.player.state(), receipt);
  check();
  const copied = copyImage(context.canvas);
  const encoding = Promise.resolve(copied?.png);
  // Observe early failures even when a later state query aborts this capture.
  encoding.catch(() => {});
  requireCapture(Number.isSafeInteger(copied?.width) && Number.isSafeInteger(copied?.height) &&
    copied.width > 0 && copied.height > 0 && copied.width <= 8192 && copied.height <= 8192 &&
    copied.width <= Math.floor(MAX_PIXELS / copied.height), "Captured image exceeds the review size limit.");
  assertPausedPublication(await context.player.state(), receipt);
  check();
  const png = await encoding;
  check();
  requireCapture(png instanceof Uint8Array && png.byteLength > 0 && png.byteLength <= MAX_PNG_BYTES,
    "The viewer could not encode a bounded PNG capture.");
  // Own the bytes; neither the canvas adapter nor later scene changes can alter
  // the accepted snapshot while its source and image digests are being computed.
  const ownedPng = new Uint8Array(png);
  const [sourceHash, imageHash] = await Promise.all([sha256(encoder.encode(source)), sha256(ownedPng)]);
  check();
  const backend = context.player.rendererBackend;
  requireCapture(typeof backend === "string" && backend.trim() &&
    !backend.includes("\0") && encoder.encode(backend).length <= 64, "Renderer identity is unavailable.");
  const frame = Object.freeze({
    id: globalThis.crypto.randomUUID(), mimeType: "image/png",
    width: copied.width, height: copied.height, byteLength: ownedPng.byteLength, sha256: imageHash,
    provenance: Object.freeze({
      sessionId: `playground:${context.runGeneration}:${receipt.session}`,
      sourceSha256: sourceHash, build: observedBuild(build),
      requestedTime: paused.time, publishedTime: receipt.time, backend,
      // A publication sequence is not a semantic scene/frame revision.
      sceneRevision: null, frameRevision: null,
    }),
  });
  return Object.freeze({ source, png: ownedPng, frame, presentationReceipt: receipt });
}

export function createLiveFeedbackExport(capture, selection, comment) {
  const { point, region = null } = selection ?? {};
  const target = point ?? region;
  const validPoint = target && [target.x, target.y].every(value => Number.isFinite(value) && value >= 0 && value <= 1);
  const validRegion = region && Number.isFinite(region.width) && Number.isFinite(region.height) &&
    region.width > 0 && region.height > 0 && region.x + region.width <= 1 && region.y + region.height <= 1;
  requireCapture(validPoint && (point ? region === null : validRegion), "Select an area or point on the captured frame.");
  requireCapture(typeof comment === "string" && comment.trim() && !comment.includes("\0") &&
    encoder.encode(comment).length <= 4000, "Write a comment of at most 4,000 UTF-8 bytes.");
  let binary = "";
  for (let offset = 0; offset < capture.png.length; offset += 0x8000) {
    binary += String.fromCharCode(...capture.png.subarray(offset, offset + 0x8000));
  }
  const time = capture.frame.provenance.publishedTime;
  return {
    kind: "noon-live-feedback-export", schemaVersion: 1,
    source: capture.source, pngBase64: btoa(binary), presentationReceipt: capture.presentationReceipt,
    feedback: {
      kind: "noon-frame-feedback", schemaVersion: 1, status: "pending_review",
      feedback: {
        interval: { start: time, end: time },
        region: region && { x: region.x, y: region.y, width: region.width, height: region.height },
        ...(point ? { point: { x: point.x, y: point.y } } : {}),
        observation: comment, desiredChange: "Propose a focused correction for this comment.", preserve: null,
      },
      frame: capture.frame,
    },
  };
}

export function copyDisplayedCanvas(canvas) {
  const { width, height } = canvas;
  requireCapture(Number.isSafeInteger(width) && Number.isSafeInteger(height) && width > 0 && height > 0 &&
    width <= 8192 && height <= 8192 && width <= Math.floor(MAX_PIXELS / height),
  "The displayed canvas is too large or unavailable for feedback.");
  const copy = canvas.ownerDocument.createElement("canvas");
  copy.width = width; copy.height = height;
  const context = copy.getContext("2d");
  requireCapture(context !== null, "Image capture is unavailable in this browser.");
  try { context.drawImage(canvas, 0, 0); }
  catch (error) { copy.width = 1; copy.height = 1; throw error; }
  const png = new Promise((resolve, reject) => {
    try {
      copy.toBlob(blob => {
        if (blob === null || blob.size > MAX_PNG_BYTES) reject(new Error("PNG capture failed or exceeded its byte limit."));
        else blob.arrayBuffer().then(bytes => resolve(new Uint8Array(bytes)), reject);
      }, "image/png");
    } catch (error) { reject(error); }
  }).finally(() => { copy.width = 1; copy.height = 1; });
  return { width, height, png };
}
