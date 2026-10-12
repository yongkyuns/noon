// Compute one localized edit so formatting preserves undo and maps selections
// through CodeMirror's normal transaction path rather than replacing the editor.
export function sourceEdit(before, after) {
  let from = 0;
  while (from < before.length && from < after.length && before[from] === after[from]) from += 1;
  let to = before.length;
  let end = after.length;
  while (to > from && end > from && before[to - 1] === after[end - 1]) { to -= 1; end -= 1; }
  return { from, to, insert: after.slice(from, end) };
}

export async function formatEditorSource(textarea, format) {
  const source = textarea.value;
  const editorDocument = textarea.editorView?.state.doc;
  const formatted = await format(source);
  if (textarea.value !== source || textarea.editorView?.state.doc !== editorDocument) return "Source changed; format again";
  if (formatted === source) return "Already formatted";
  const changes = sourceEdit(source, formatted);
  if (textarea.editorView) {
    textarea.editorView.dispatch({ changes, userEvent: "input.format" });
  } else {
    textarea.setRangeText(changes.insert, changes.from, changes.to, "preserve");
    textarea.dispatchEvent(new Event("input", { bubbles: true }));
  }
  return "Formatted with Ruff";
}

export function installPythonEditorTools(textarea, format) {
  const toolbar = document.createElement("div");
  toolbar.className = "python-editor-tools";
  toolbar.setAttribute("aria-label", "Python editor tools");
  const makeButton = (label, title) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "secondary-button";
    button.textContent = label;
    button.title = title;
    toolbar.append(button);
    return button;
  };
  const formatButton = makeButton("Format", "Format Python with Ruff · Shift+Alt+F");
  const wrapButton = makeButton("Wrap: off", "Toggle visual line wrapping; source is unchanged");
  wrapButton.setAttribute("aria-pressed", "false");
  const saveButton = makeButton("Save .py", "Download the current Python source");
  const help = document.createElement("details");
  help.className = "python-editor-help";
  const summary = document.createElement("summary");
  summary.textContent = "Shortcuts";
  const helpText = document.createElement("div");
  helpText.textContent = "Ctrl/Cmd+Enter: run now · Shift+Alt+F: format · In the enhanced editor: Ctrl/Cmd+F: find and replace · Ctrl/Cmd+[: indent less · Ctrl/Cmd+]: indent more · Ctrl/Cmd+Z: undo. Edits restart the preview after a typing pause.";
  help.append(summary, helpText);
  toolbar.append(help);
  const output = document.createElement("output");
  output.className = "python-editor-tool-status";
  output.setAttribute("aria-live", "polite");
  toolbar.append(output);
  textarea.before(toolbar);

  let wrapped = false;
  let editorHost = null;
  function updateWrap() {
    textarea.wrap = wrapped ? "soft" : "off";
    textarea.style.whiteSpace = wrapped ? "pre-wrap" : "pre";
    if (editorHost) editorHost.dataset.wrap = String(wrapped);
    textarea.editorView?.requestMeasure();
    wrapButton.textContent = wrapped ? "Wrap: on" : "Wrap: off";
    wrapButton.setAttribute("aria-pressed", String(wrapped));
  }
  updateWrap();
  wrapButton.addEventListener("click", () => { wrapped = !wrapped; updateWrap(); });
  async function runFormat() {
    if (formatButton.disabled) return;
    formatButton.disabled = true;
    output.value = "Loading formatter…";
    try {
      output.value = await formatEditorSource(textarea, format);
    } catch (error) {
      output.value = `Format failed: ${error.message ?? error}`;
    } finally {
      formatButton.disabled = false;
    }
  }
  formatButton.addEventListener("click", () => { void runFormat(); });
  textarea.parentElement.addEventListener("keydown", (event) => {
    if (event.shiftKey && event.altKey && event.key.toLowerCase() === "f") {
      event.preventDefault();
      void runFormat();
    }
  });
  saveButton.addEventListener("click", () => {
    const url = URL.createObjectURL(new Blob([textarea.value], { type: "text/x-python;charset=utf-8" }));
    const link = document.createElement("a");
    link.href = url;
    link.download = "main.py";
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
  });
  return { attach(host) { editorHost = host; updateWrap(); } };
}
