export const EXECUTION_VISIBILITY_CHANNEL = "noon.execution.visibility";
export const EXECUTION_VISIBILITY_VERSION = 1;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export function executionVisibilityMetadata(json) {
  if (typeof json !== "string") {
    throw new TypeError("execution visibility must be a JSON string");
  }

  let visibility;
  try {
    visibility = JSON.parse(json);
  } catch (error) {
    throw new Error(`execution visibility is invalid JSON: ${error.message}`);
  }

  if (!isRecord(visibility) || visibility.channel !== EXECUTION_VISIBILITY_CHANNEL) {
    throw new Error("execution visibility has an invalid channel");
  }
  if (visibility.protocol_version !== EXECUTION_VISIBILITY_VERSION) {
    throw new Error(`unsupported execution visibility version ${visibility.protocol_version}`);
  }
  if (!Number.isFinite(visibility.time)) {
    throw new Error("execution visibility has an invalid time");
  }
  if (!Number.isSafeInteger(visibility.layout_generation) || visibility.layout_generation < 0) {
    throw new Error("execution visibility has an invalid layout generation");
  }
  if (!Number.isSafeInteger(visibility.total_live) || visibility.total_live < 0) {
    throw new Error("execution visibility has an invalid live-object count");
  }
  if (!Array.isArray(visibility.slots)) {
    throw new Error("execution visibility slots must be an array");
  }

  return {
    time: visibility.time,
    layoutGeneration: visibility.layout_generation,
    totalLive: visibility.total_live,
  };
}

export function encodeTransferableExecutionVisibility({ session, sequence }, json) {
  validateFrameIdentity(session, sequence);
  executionVisibilityMetadata(json);
  const payload = encoder.encode(json);
  const buffer = payload.buffer.slice(payload.byteOffset, payload.byteOffset + payload.byteLength);
  return {
    message: {
      type: "execution_visibility",
      session,
      sequence,
      buffer,
    },
    transfer: [buffer],
  };
}

export function decodeTransferableExecutionVisibility(message) {
  if (!isRecord(message) || message.type !== "execution_visibility") {
    throw new Error("execution visibility message must be an execution_visibility envelope");
  }
  validateFrameIdentity(message.session, message.sequence);
  if (!(message.buffer instanceof ArrayBuffer)) {
    throw new Error("execution visibility payload must be an ArrayBuffer");
  }

  const json = decoder.decode(new Uint8Array(message.buffer));
  return {
    json,
    frame: { session: message.session, sequence: message.sequence },
    visibility: executionVisibilityMetadata(json),
  };
}

export function executionFrameKey({ session, sequence }) {
  validateFrameIdentity(session, sequence);
  return `${session}:${sequence}`;
}

function validateFrameIdentity(session, sequence) {
  if (!Number.isSafeInteger(session) || session < 0) {
    throw new Error("execution visibility has an invalid session");
  }
  if (!Number.isSafeInteger(sequence) || sequence < 0) {
    throw new Error("execution visibility has an invalid sequence");
  }
}

function isRecord(value) {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
