import { parseAuthoringDiagnostic } from "./authoring-diagnostic.js";

// Copy only an explicit, invocation-local Python failure snapshot. Never inspect
// interpreter-global last-error state: another source/callback may have run.
export function attachAuthoringDiagnostic(error, globals) {
  try {
    const json = globals.get("__noon_diagnostic_json");
    const diagnostic = typeof json === "string" ? parseAuthoringDiagnostic(JSON.parse(json)) : null;
    if (diagnostic !== null) error.authoringDiagnostic = diagnostic;
  } catch {
    // A malformed snapshot, broken proxy, or frozen JS error must not replace
    // the original exception or interfere with the existing fatal-engine path.
  }
  return error;
}

export function authoringErrorFromMessage(message, request = null) {
  const details = [message.message, message.diagnostic?.stack].filter(Boolean);
  const error = new Error(String(details.join("\n") || "Python authoring failed"));
  const diagnostic = parseAuthoringDiagnostic(message.authoringDiagnostic);
  if (diagnostic !== null && request?.sourceDocument &&
      diagnostic.document.filename === request.sourceDocument.filename &&
      diagnostic.document.revision === request.sourceDocument.revision &&
      typeof request.source === "string") {
    error.authoringDiagnostic = diagnostic;
    error.authoringSource = request.source;
  }
  return error;
}
