const KNOWN_RUNTIME_OWNERS = new Map([
  ["engine", "engine worker"],
  ["render", "render worker"],
  ["host", "host callback"],
]);

export function formatPlaygroundRuntimeError(
  error,
  { owner = null, backend = "", executionMode = "" } = {},
) {
  const message = error instanceof Error ? error.message : String(error);
  const context = [];
  if (typeof backend === "string" && backend.trim() !== "") {
    context.push(backend.trim());
  }
  if (KNOWN_RUNTIME_OWNERS.has(owner)) {
    context.push(KNOWN_RUNTIME_OWNERS.get(owner));
  }
  if (typeof executionMode === "string" && executionMode.trim() !== "") {
    context.push(`(${executionMode.trim()})`);
  }
  return context.length === 0 ? message : `${context.join(" ")}: ${message}`;
}

export function runtimeErrorDiagnostics(
  { owner = null, backend = "", executionMode = "" } = {},
) {
  return Object.freeze({
    owner: KNOWN_RUNTIME_OWNERS.has(owner) ? owner : "",
    backend: typeof backend === "string" ? backend.trim() : "",
    executionMode: typeof executionMode === "string" ? executionMode.trim() : "",
  });
}
