import {
  authoringDiagnosticSourceRange, presentAuthoringDiagnostic,
} from "./authoring-diagnostic.js";

// Presentation only. Generation suppression and engine recovery still belong
// to the existing run router and runtime clients. No scene state is cached here.
export class PlaygroundDiagnostics {
  #editor;
  #details;
  #traceback;
  #panel;
  #title;
  #message;
  #hint;
  #operation;
  #sourceButton;
  #shown = null;
  #onEdit;

  constructor(editor, details, traceback) {
    this.#editor = editor;
    this.#details = details;
    this.#traceback = traceback;
    const document = editor.ownerDocument;
    const element = (tag, className) => {
      const node = document.createElement(tag);
      node.className = className;
      return node;
    };
    this.#panel = element("section", "playground-diagnostic");
    this.#panel.id = "authoring-diagnostic";
    this.#panel.hidden = true;
    this.#panel.setAttribute("aria-label", "Animation error");
    this.#panel.setAttribute("aria-live", "polite");
    this.#title = element("strong", "diagnostic-title");
    this.#message = element("p", "diagnostic-message");
    this.#operation = element("p", "diagnostic-operation");
    this.#hint = element("p", "diagnostic-hint");
    this.#sourceButton = element("button", "secondary-button diagnostic-source");
    this.#sourceButton.type = "button";
    this.#sourceButton.addEventListener("click", () => this.#navigate());
    this.#panel.append(this.#title, this.#message, this.#operation, this.#hint, this.#sourceButton);
    details.before(this.#panel);
    const summary = details.querySelector("summary");
    if (summary) summary.textContent = "Technical details";
    this.#onEdit = () => this.clear();
    editor.addEventListener("input", this.#onEdit);
  }

  clear() {
    this.#shown = null;
    this.#panel.hidden = true;
    this.#sourceButton.hidden = true;
    this.#details.hidden = true;
    this.#details.open = false;
    this.#traceback.textContent = "";
  }

  show(error, fallbackTitle = "Animation could not run") {
    this.clear();
    const view = presentAuthoringDiagnostic(error?.authoringDiagnostic);
    const title = view?.title ?? fallbackTitle;
    this.#title.textContent = title;
    // Literal text only. The complete raw stack is retained below, not parsed
    // to manufacture classifications or source locations.
    this.#message.textContent = view?.message ?? String(error?.message ?? error).split("\n")[0].slice(0, 2048);
    this.#hint.textContent = view?.hint ?? "Inspect the technical details. Your source remains in the editor.";
    this.#operation.textContent = [view?.operation, view?.code].filter(Boolean).join(" · ");
    this.#operation.hidden = !this.#operation.textContent;
    this.#traceback.textContent = String(error?.stack ?? error);
    if (view?.truncated) this.#traceback.textContent += "\n\nStructured diagnostic was truncated; original stack is shown above.";
    if (view && typeof error.authoringSource === "string" && this.#editor.value === error.authoringSource) {
      this.#shown = { diagnostic: error.authoringDiagnostic, source: error.authoringSource, document: view.document };
      const range = this.#range();
      this.#sourceButton.hidden = range === null;
      this.#sourceButton.textContent = range === null ? "" : `Go to ${view.document.filename}:${view.location.line}`;
    }
    this.#details.hidden = false;
    this.#details.open = false;
    this.#panel.hidden = false;
    return title;
  }

  #range() {
    const shown = this.#shown;
    if (shown === null || this.#editor.value !== shown.source) return null;
    return authoringDiagnosticSourceRange(shown.diagnostic, shown.source, shown.document);
  }

  #navigate() {
    const range = this.#range();
    if (range === null) return;
    const view = this.#editor.editorView;
    if (view) {
      view.dispatch({ selection: { anchor: range.from, head: range.to }, scrollIntoView: true });
      view.focus();
    } else {
      this.#editor.focus();
      this.#editor.setSelectionRange(range.from, range.to);
    }
  }

  dispose() {
    this.clear();
    this.#editor.removeEventListener("input", this.#onEdit);
    this.#panel.remove();
  }
}
