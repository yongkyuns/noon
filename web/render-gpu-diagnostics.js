export function drainRendererGpuDiagnostics(renderer, { onRecoverable, onFatal }) {
  if (!renderer || typeof renderer.takeGpuDiagnosticJson !== "function") {
    return true;
  }
  if (typeof onRecoverable !== "function" || typeof onFatal !== "function") {
    throw new TypeError("renderer GPU diagnostics require recoverable and fatal handlers");
  }

  while (true) {
    const raw = renderer.takeGpuDiagnosticJson();
    if (raw === undefined || raw === null) {
      return true;
    }
    const diagnostic = parseGpuDiagnostic(raw);
    if (diagnostic.severity === "recoverable") {
      onRecoverable(diagnostic);
      continue;
    }
    onFatal(diagnostic);
    return false;
  }
}

export function formatGpuDiagnostic(diagnostic) {
  const generation = Number.isSafeInteger(diagnostic?.generation)
    ? ` generation ${diagnostic.generation}`
    : "";
  const backend = diagnostic?.backend || "GPU";
  const kind = diagnostic?.kind ? ` ${diagnostic.kind}` : "";
  const message = diagnostic?.message ? `: ${diagnostic.message}` : "";
  return `${backend}${generation}${kind}${message}`;
}

function parseGpuDiagnostic(raw) {
  let diagnostic;
  try {
    diagnostic = JSON.parse(raw);
  } catch (error) {
    throw new Error(`renderer returned invalid GPU diagnostic JSON: ${error}`);
  }
  if (!diagnostic || typeof diagnostic !== "object") {
    throw new Error("renderer GPU diagnostic must be an object");
  }
  if (!Number.isSafeInteger(diagnostic.generation) || diagnostic.generation <= 0) {
    throw new Error("renderer GPU diagnostic generation must be a positive safe integer");
  }
  if (typeof diagnostic.backend !== "string" || diagnostic.backend === "") {
    throw new Error("renderer GPU diagnostic backend must be non-empty");
  }
  if (!["validation", "out_of_memory", "internal"].includes(diagnostic.kind)) {
    throw new Error(`renderer GPU diagnostic has unknown kind ${diagnostic.kind}`);
  }
  if (!["recoverable", "fatal"].includes(diagnostic.severity)) {
    throw new Error(`renderer GPU diagnostic has unknown severity ${diagnostic.severity}`);
  }
  if (typeof diagnostic.message !== "string" || diagnostic.message === "") {
    throw new Error("renderer GPU diagnostic message must be non-empty");
  }
  if (diagnostic.kind === "validation" && diagnostic.severity !== "recoverable") {
    throw new Error("renderer validation diagnostics must be recoverable");
  }
  if (diagnostic.kind !== "validation" && diagnostic.severity !== "fatal") {
    throw new Error("renderer OOM/internal diagnostics must be fatal");
  }
  return diagnostic;
}
