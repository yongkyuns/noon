export function drainRendererGpuErrors(renderer, { onRecoverable, onFatal }) {
  if (!renderer || typeof renderer.takeGpuUncapturedErrorJson !== "function") {
    return true;
  }
  if (typeof onRecoverable !== "function" || typeof onFatal !== "function") {
    throw new TypeError("renderer GPU error routing requires recoverable and fatal handlers");
  }

  while (true) {
    const raw = renderer.takeGpuUncapturedErrorJson();
    if (raw === undefined || raw === null) {
      return true;
    }
    const error = parseGpuError(raw, renderer.rendererBackend());
    if (error.fatal) {
      onFatal(error);
      return false;
    }
    onRecoverable(error);
  }
}

function parseGpuError(raw, backend) {
  let error;
  try {
    error = JSON.parse(raw);
  } catch (cause) {
    throw new Error(`renderer returned invalid GPU error JSON: ${cause}`);
  }
  if (!error || typeof error !== "object") {
    throw new Error("renderer GPU error must be an object");
  }
  if (!Number.isSafeInteger(error.generation) || error.generation <= 0) {
    throw new Error("renderer GPU error generation must be a positive safe integer");
  }
  if (!["validation", "out_of_memory", "internal"].includes(error.kind)) {
    throw new Error(`renderer GPU error has unknown kind ${error.kind}`);
  }
  if (typeof error.fatal !== "boolean") {
    throw new Error("renderer GPU error fatal flag must be boolean");
  }
  if (typeof error.message !== "string" || error.message === "") {
    throw new Error("renderer GPU error message must be non-empty");
  }
  const expectedFatal = error.kind !== "validation";
  if (error.fatal !== expectedFatal) {
    throw new Error(`renderer GPU error has inconsistent fatal flag for ${error.kind}`);
  }
  return Object.freeze({ ...error, backend });
}
