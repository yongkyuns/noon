import { parseAuthoringDiagnostic } from "./authoring-diagnostic.js";

/** A document revision is host metadata, not a semantic/engine identity. */
export function authoringSourceDocument(context, requestId) {
  const playground = context?.playground;
  const valid = value => Number.isSafeInteger(value) && value >= 0;
  let revision;
  if (valid(playground?.selection_generation) && valid(playground?.run_generation)) {
    revision = `s${playground.selection_generation}-r${playground.run_generation}`;
  } else if (valid(requestId)) {
    revision = `request-${requestId}`;
  } else {
    throw new TypeError("Authoring diagnostics require a source run identity");
  }
  return Object.freeze({ filename: `noon-${revision}.py`, revision });
}

const diagnosticScope = phase => `from _noon_diagnostics import source_diagnostic_scope
with source_diagnostic_scope(
    source=__noon_source, filename=__noon_filename, revision=__noon_revision,
    phase="${phase}", on_error=__noon_capture_diagnostic,
):`;

// This is the actual existing portable compiler, not a second parser or source
// execution path. Compile once before package discovery so malformed source
// gets a source-linked SyntaxError, rather than a dependency-parser traceback.
const COMPILE_SOURCE = `${diagnosticScope("compile")}
    from _manim_source_execution import compile_authoring_source
    __noon_code, __noon_portable_constructs = compile_authoring_source(
        __noon_source, filename=__noon_filename
    )
`;

/** Wrap only the fixed host wrapper; authored source is never indented/rewritten. */
export function diagnosedHostCode(hostCode) {
  return `${diagnosticScope("construct")}\n${hostCode.split("\n").map(line => `    ${line}`).join("\n")}\n__noon_result_json`;
}

/** Per-request Python globals and exception snapshot, preserving the original error. */
export async function withAuthoringDiagnostics(pyodide, source, context, requestId, execute) {
  const document = authoringSourceDocument(context, requestId);
  const dictConstructor = pyodide.globals.get("dict");
  let globals;
  try {
    globals = dictConstructor();
  } finally {
    dictConstructor.destroy();
  }
  let captured = null;
  let failed = false;
  try {
    globals.set("__noon_source", source);
    globals.set("__noon_context_json", JSON.stringify(context));
    globals.set("__noon_filename", document.filename);
    globals.set("__noon_revision", document.revision);
    globals.set("__noon_capture_diagnostic", json => { captured = json; });
    pyodide.runPython(COMPILE_SOURCE, { globals });
    return await execute(globals);
  } catch (error) {
    failed = true;
    try {
      // A host may rethrow the same Error on a later run. Clear our prior facts
      // before parsing this invocation, including malformed/absent captures.
      Object.defineProperty(error, "authoringDiagnostic", { value: undefined, configurable: true });
      const diagnostic = typeof captured === "string"
        ? parseAuthoringDiagnostic(JSON.parse(captured)) : null;
      if (diagnostic !== null && diagnostic.document.filename === document.filename &&
          diagnostic.document.revision === document.revision) {
        Object.defineProperty(error, "authoringDiagnostic", { value: diagnostic, configurable: true });
      }
    } catch {
      // Includes frozen/primitive errors and failed diagnostics. Keep the
      // original object so existing fatal WASM and worker retirement still run.
    }
    throw error;
  } finally {
    try {
      globals.destroy();
    } catch (cleanupError) {
      if (!failed) throw cleanupError;
    }
  }
}

/** Optional v1 facts; original message and stack stay on their existing wire path. */
export function authoringDiagnosticPayload(error) {
  try {
    const diagnostic = parseAuthoringDiagnostic(error?.authoringDiagnostic);
    return diagnostic === null ? {} : { authoringDiagnostic: diagnostic };
  } catch {
    return {};
  }
}
