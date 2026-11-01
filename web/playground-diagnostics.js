import {
  parseAuthoringDiagnostic, presentAuthoringDiagnostic, authoringDiagnosticSourceRange,
} from "./authoring-diagnostic.js";

// When structured metadata is unavailable (for example at Python startup or
// during a dependency fetch), retain the original error's first useful line.
// This only displays facts; it never guesses error category, scope or recovery.
export function fallbackFailureMessage(error) {
  const unavailable = "Noon could not complete this operation.";
  let raw;
  try {
    raw = typeof error?.message === "string" ? error.message : String(error);
  } catch {
    return unavailable;
  }
  const firstLine = raw.slice(0, 4096).split(/\r\n|\r|\n/).find(line => line.trim().length > 0);
  if (!firstLine) return unavailable;
  // Do not cut an emoji/surrogate pair. Full context remains in Technical details.
  return [...firstLine].slice(0, 400).join("");
}

export function failureTechnicalDetails(error) {
  try {
    const stack = String(error?.stack ?? error);
    const message = typeof error?.message === "string" ? error.message : "";
    // Safari stacks can omit the message, which contains the Python traceback.
    return message && !stack.includes(message) ? `${message}\n\n${stack}` : stack;
  } catch {
    return "Technical details are unavailable for this failure.";
  }
}

/** Presentation only. Does not restart workers, decide recovery, or mutate a scene. */
export function createPlaygroundDiagnostics({ editor, details, traceback }) {
  const document = editor.ownerDocument;
  const element = (tag, className) => {
    const node = document.createElement(tag);
    if (className) node.className = className;
    return node;
  };
  const panel = element("section", "authoring-problem");
  panel.id = "authoring-problem";
  panel.hidden = true;
  panel.setAttribute("aria-live", "polite");
  panel.setAttribute("aria-label", "Source diagnostic");
  const title = element("strong");
  const message = element("p");
  const hint = element("p");
  const operation = element("p");
  const excerpt = element("pre");
  const navigate = element("button", "secondary-button");
  navigate.type = "button";
  navigate.setAttribute("aria-controls", editor.id);
  const truncated = element("p");
  truncated.textContent = "Some diagnostic details were omitted. The original traceback is available below.";
  panel.append(title, message, hint, operation, excerpt, navigate, truncated);
  details.before(panel);
  const summary = details.querySelector("summary");
  if (summary) summary.textContent = "Technical details";

  const style = element("style");
  style.textContent = `
    .authoring-problem { max-height: 13rem; overflow: auto; padding: .65rem 0; }
    .authoring-problem p { margin: .35rem 0; overflow-wrap: anywhere; }
    .authoring-problem strong { color: var(--danger, currentColor); }
    .authoring-problem pre { max-height: 4.5rem; overflow: auto; margin: .45rem 0;
      white-space: pre-wrap; overflow-wrap: anywhere; font: .75rem ui-monospace, monospace; }
    .authoring-problem + details { max-height: 12rem; overflow: auto; }
  `;
  document.head.append(style);

  let activeDocument = null;
  let executedSource = null;
  let diagnostic = null;
  const hide = () => {
    diagnostic = null;
    panel.hidden = true;
    details.hidden = true;
    details.open = false;
    traceback.textContent = "";
    excerpt.textContent = "";
  };
  const invalidate = () => {
    activeDocument = null;
    executedSource = null;
    hide();
  };
  const currentRange = () => {
    if (executedSource === null || editor.value !== executedSource) return null;
    return authoringDiagnosticSourceRange(diagnostic, executedSource, activeDocument);
  };
  const jumpToSource = () => {
    const range = currentRange();
    if (range === null) { invalidate(); return; }
    const view = editor.editorView;
    if (view) {
      view.dispatch({ selection: { anchor: range.from, head: range.to }, scrollIntoView: true });
      view.focus();
    } else {
      editor.focus();
      editor.setSelectionRange(range.from, range.to);
    }
  };
  navigate.addEventListener("click", jumpToSource);
  // CodeMirror forwards committed changes through this same textarea event.
  editor.addEventListener("input", invalidate);

  return Object.freeze({
    beginRun(nextDocument, source) {
      invalidate();
      activeDocument = Object.freeze({ ...nextDocument });
      executedSource = source;
    },
    invalidate,
    clear: hide,
    show(error, fallbackTitle = "Scene failed") {
      let next = null;
      try { next = parseAuthoringDiagnostic(error?.authoringDiagnostic); } catch {}
      if (next !== null && (activeDocument === null || editor.value !== executedSource ||
          next.document.filename !== activeDocument.filename ||
          next.document.revision !== activeDocument.revision)) {
        // Do not let an old reply replace a newer run's panel or mark new code.
        return null;
      }
      hide();
      diagnostic = next;
      const presentation = presentAuthoringDiagnostic(next);
      title.textContent = presentation?.title ?? fallbackTitle;
      message.textContent = presentation?.message ?? fallbackFailureMessage(error);
      hint.textContent = presentation?.hint ?? "Your source is unchanged. Open the technical details to identify the failure.";
      operation.textContent = [presentation?.operation, presentation?.code].filter(Boolean).join(" · ");
      operation.hidden = !operation.textContent;
      truncated.hidden = presentation?.truncated !== true;
      const range = currentRange();
      navigate.hidden = range === null;
      excerpt.hidden = range === null;
      if (range !== null) {
        const line = presentation.location.line;
        navigate.textContent = `Go to line ${line}`;
        // Use the exact source revision; never echo HTML or evaluate a fix.
        const sourceLine = executedSource.split(/\r\n|\r|\n/)[line - 1];
        excerpt.textContent = `${line}  ${sourceLine.length > 512 ? sourceLine.slice(0, 512) + "…" : sourceLine}`;
      }
      traceback.textContent = failureTechnicalDetails(error);
      details.hidden = false;
      details.open = false;
      panel.hidden = false;
      return title.textContent;
    },
    dispose() {
      invalidate();
      editor.removeEventListener("input", invalidate);
      navigate.removeEventListener("click", jumpToSource);
      panel.remove();
      style.remove();
    },
  });
}
