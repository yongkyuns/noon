// Python exception facts at the real worker/UI boundary, owned by #2047.
// This module neither parses traceback text nor decides engine recovery policy.
const PHASES = new Set(["startup", "dependencies", "compile", "construct", "callback", "runtime"]);
const RELATIONS = new Set(["cause", "context", "callback"]);
const record = (value) => value !== null && typeof value === "object" && !Array.isArray(value);
const integer = (value, min) => Number.isSafeInteger(value) && value >= min;
const freeze = Object.freeze;

/** Validate and copy a bounded v1 Python snapshot. Malformed input stays unknown. */
export function parseAuthoringDiagnostic(value) {
  let remaining = 8;
  const active = new Set();
  const fail = () => { throw new TypeError("Invalid authoring diagnostic"); };
  const text = (value, limit, nonempty = false) => {
    if (typeof value !== "string" || value.length > limit || (nonempty && !value.length)) fail();
    return value;
  };
  const array = (value, limit, project) => {
    if (!Array.isArray(value) || value.length > limit) fail();
    return freeze(value.map(project));
  };
  const location = (value) => {
    if (value === null) return null;
    if (!record(value) || !integer(value.line, 1) ||
        !(value.column === null || integer(value.column, 0))) fail();
    if (value.endLine === null && value.endColumn === null) {
      return freeze({ line: value.line, column: value.column, endLine: null, endColumn: null });
    }
    if (value.column === null || !integer(value.endLine, value.line) ||
        !integer(value.endColumn, 0) ||
        (value.endLine === value.line && value.endColumn < value.column)) fail();
    return freeze({ line: value.line, column: value.column,
      endLine: value.endLine, endColumn: value.endColumn });
  };
  const engine = (value) => {
    if (value === null) return null;
    if (!record(value)) fail();
    return freeze({
      category: text(value.category, 128), code: text(value.code, 128),
      operation: value.operation === null ? null : text(value.operation, 256),
      causes: array(value.causes, 8, (cause) => {
        if (!record(cause)) fail();
        return freeze({ category: text(cause.category, 128),
          code: text(cause.code, 128), message: text(cause.message, 512) });
      }),
    });
  };
  const exception = (value) => {
    if (!record(value) || remaining-- <= 0 || active.has(value)) fail();
    active.add(value);
    const result = {
      type: text(value.type, 128, true), module: text(value.module, 128),
      message: text(value.message, 2048), location: location(value.location),
      frames: array(value.frames, 16, (frame) => {
        if (!record(frame) || !(frame.line === null || integer(frame.line, 1)) ||
            typeof frame.userSource !== "boolean") fail();
        return freeze({ filename: text(frame.filename, 512),
          function: text(frame.function, 128), line: frame.line, userSource: frame.userSource });
      }),
      engine: engine(value.engine), notes: array(value.notes, 4, (note) => text(note, 512)),
      cause: value.cause === null ? null : exception(value.cause),
      causeRelation: value.causeRelation,
      children: array(value.children, 8, exception),
    };
    if (result.cause === null ? result.causeRelation !== null : !RELATIONS.has(result.causeRelation)) fail();
    active.delete(value);
    return freeze(result);
  };
  try {
    if (!record(value) || value.version !== 1 || !PHASES.has(value.phase) ||
        !record(value.document) || typeof value.truncated !== "boolean") return null;
    return freeze({
      version: 1, phase: value.phase,
      document: freeze({ filename: text(value.document.filename, 512, true),
        revision: text(value.document.revision, 128, true) }),
      exception: exception(value.exception), truncated: value.truncated,
    });
  } catch {
    // A malformed diagnostic must not convert an ordinary source failure into
    // a worker/protocol crash. The caller retains the original raw error.
    return null;
  }
}

function primaryLocation(exception) {
  // Prefer the actual chained Python failure (e.g. callback source) over its
  // wrapper callsite. Do not choose an arbitrary child of an exception group.
  return (exception.cause && primaryLocation(exception.cause)) || exception.location;
}

/** Text-only view model. Render with textContent, never as HTML. */
export function presentAuthoringDiagnostic(value) {
  const diagnostic = parseAuthoringDiagnostic(value);
  if (diagnostic === null) return null;
  const error = diagnostic.exception;
  let title = `Python raised ${error.type}`;
  let hint = "Inspect the source location and technical details to identify the cause.";
  switch (error.engine?.category) {
    case "unsupported_operation":
      title = "Operation not supported";
      hint = "Check this operation's supported arguments and execution context.";
      break;
    case "invalid_input":
      title = "Noon rejected an input";
      hint = "Check the value passed to the reported operation.";
      break;
    case "missing_resource":
      title = "Required resource unavailable";
      hint = "Check that the required resource is available before this operation.";
      break;
    case "callback_failure":
      title = "A callback failed";
      hint = "Inspect the callback's source and original exception in the technical details.";
      break;
  }
  return freeze({ title, message: error.message, hint,
    operation: error.engine?.operation ?? null, code: error.engine?.code ?? null,
    location: primaryLocation(error), document: diagnostic.document,
    truncated: diagnostic.truncated });
}

/** Safe editor range; callers supply their CURRENT document identity and text. */
export function authoringDiagnosticSourceRange(value, source, document) {
  const diagnostic = parseAuthoringDiagnostic(value);
  if (diagnostic === null || typeof source !== "string" ||
      diagnostic.document.filename !== document?.filename ||
      diagnostic.document.revision !== document?.revision) return null;
  const position = primaryLocation(diagnostic.exception);
  if (position === null) return null;
  const starts = [0];
  const ends = [];
  for (const match of source.matchAll(/\r\n|\r|\n/g)) {
    ends.push(match.index);
    starts.push(match.index + match[0].length);
  }
  ends.push(source.length);
  const offset = (line, column) => {
    const start = starts[line - 1];
    if (start === undefined || column > ends[line - 1] - start) return null;
    const absolute = start + column;
    // UTF-16 offsets are not permission to split a surrogate pair. A corrupt
    // payload must not underline half of a character in otherwise valid source.
    if (column > 0 && absolute < ends[line - 1] &&
        source.charCodeAt(absolute) >= 0xDC00 && source.charCodeAt(absolute) <= 0xDFFF &&
        source.charCodeAt(absolute - 1) >= 0xD800 && source.charCodeAt(absolute - 1) <= 0xDBFF) return null;
    return absolute;
  };
  const from = offset(position.line, position.column ?? 0);
  const to = position.endLine === null ? ends[position.line - 1]
    : offset(position.endLine, position.endColumn);
  if (from === null || to === null || to === undefined || to < from) return null;
  return freeze({ from, to });
}
