const CODEMIRROR_URL = "https://esm.sh/codemirror@6.0.2";
const PYTHON_URL = "https://esm.sh/@codemirror/lang-python@6.2.1";
const LINT_URL = "https://esm.sh/@codemirror/lint@6.9.7";
const THEME_URL = "https://esm.sh/@codemirror/theme-one-dark@6.1.3";
const RUFF_URL = "https://esm.sh/@astral-sh/ruff-wasm-web@0.16.4";

let ruffWorkspacePromise = null;

if (typeof document !== "undefined") {
  await enhancePythonEditors();
}

async function enhancePythonEditors() {
  const textareas = [
    document.querySelector("#python-scene-source"),
    document.querySelector("#python-source"),
  ].filter(Boolean);
  if (textareas.length === 0) {
    return;
  }

  const [{ EditorView, basicSetup }, { python }, { linter, lintGutter }, { oneDark }] =
    await Promise.all([
      import(CODEMIRROR_URL),
      import(PYTHON_URL),
      import(LINT_URL),
      import(THEME_URL),
    ]);

  const editorTheme = EditorView.theme({
    "&": {
      height: "100%",
      minHeight: "0",
      backgroundColor: "#080b12",
      fontSize: "0.82rem",
    },
    ".cm-scroller": {
      overflow: "auto",
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      lineHeight: "1.58",
    },
    ".cm-content": { padding: "0.35rem 0" },
    ".cm-gutters": {
      backgroundColor: "#0b0f18",
      borderRight: "1px solid #171f2e",
      color: "#58647a",
    },
    ".cm-activeLineGutter": { backgroundColor: "#141a27" },
    ".cm-activeLine": { backgroundColor: "rgba(142, 124, 255, 0.055)" },
    ".cm-lintRange-error": {
      backgroundImage:
        "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='6' height='3'%3E%3Cpath d='M0 2.5 1.5 1 3 2.5 4.5 1 6 2.5' fill='none' stroke='%23ff8f9d' stroke-width='1'/%3E%3C/svg%3E\")",
    },
    ".cm-lintRange-warning": {
      backgroundImage:
        "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='6' height='3'%3E%3Cpath d='M0 2.5 1.5 1 3 2.5 4.5 1 6 2.5' fill='none' stroke='%23e8c36a' stroke-width='1'/%3E%3C/svg%3E\")",
    },
  });

  for (const textarea of textareas) {
    const host = document.createElement("div");
    host.className = "python-code-editor";
    textarea.before(host);
    textarea.hidden = true;

    const view = new EditorView({
      doc: textarea.value,
      parent: host,
      extensions: [
        basicSetup,
        python(),
        oneDark,
        editorTheme,
        lintGutter(),
        linter(runRuff, { delay: 300 }),
        EditorView.lineWrapping,
      ],
    });

    // Keep the hidden textarea as the stable integration surface for the
    // playground. Programmatic .value writes are projected into CodeMirror,
    // while real user edits emit the textarea input event expected by gallery
    // draft/reset state and any other existing consumers.
    view.contentDOM.addEventListener("input", () => {
      textarea.dispatchEvent(new Event("input", { bubbles: true }));
    });

    Object.defineProperty(textarea, "value", {
      configurable: true,
      get() {
        return view.state.doc.toString();
      },
      set(value) {
        const next = String(value ?? "");
        const current = view.state.doc.toString();
        if (next === current) {
          return;
        }
        view.dispatch({
          changes: { from: 0, to: view.state.doc.length, insert: next },
        });
      },
    });

    textarea.editorView = view;
    host.dataset.editorReady = "true";
  }

  const style = document.createElement("style");
  style.textContent = `
    .python-code-editor {
      width: 100%;
      min-height: 0;
      flex: 1;
      overflow: hidden;
      background: #080b12;
    }
    .python-code-editor .cm-editor { height: 100%; }
    .python-code-editor .cm-focused { outline: 2px solid #aa9cff; outline-offset: -2px; }
    @media (min-width: 68.01rem) {
      .canvas-wrap {
        padding: 0 !important;
        place-items: stretch !important;
      }
      .canvas-wrap > canvas {
        width: 100%;
        height: 100%;
        max-width: none;
        aspect-ratio: auto;
        border: 0;
        border-radius: 0;
      }
    }
    @media (max-width: 44rem) {
      .python-code-editor { min-height: 25rem; }
      .python-code-editor .cm-editor { font-size: 0.76rem; }
    }
  `;
  document.head.append(style);
}

async function runRuff(view) {
  try {
    const workspace = await getRuffWorkspace();
    const diagnostics = workspace.check(view.state.doc.toString());
    return diagnostics.map((diagnostic) => toCodeMirrorDiagnostic(view.state.doc, diagnostic));
  } catch (error) {
    console.warn("Ruff linting unavailable", error);
    return [];
  }
}

async function getRuffWorkspace() {
  ruffWorkspacePromise ??= import(RUFF_URL).then(async (ruff) => {
    await ruff.default();
    return new ruff.Workspace(
      {
        "line-length": 88,
        "indent-width": 4,
        lint: { select: ["E4", "E7", "E9", "F"] },
      },
      ruff.PositionEncoding.Utf16,
    );
  });
  return ruffWorkspacePromise;
}

function toCodeMirrorDiagnostic(doc, diagnostic) {
  const from = sourceLocationToOffset(doc, diagnostic.start_location);
  const rawTo = sourceLocationToOffset(doc, diagnostic.end_location);
  const to = Math.max(from, rawTo);
  const code = diagnostic.code ? `${diagnostic.code}: ` : "";
  const severity = diagnostic.code?.startsWith("E9") ? "error" : "warning";
  return {
    from,
    to,
    severity,
    message: `${code}${diagnostic.message}`,
    source: "Ruff",
  };
}

function sourceLocationToOffset(doc, location) {
  const row = Math.min(Math.max(Number(location?.row) || 1, 1), doc.lines);
  const line = doc.line(row);
  const column = Math.max((Number(location?.column) || 1) - 1, 0);
  return Math.min(line.from + column, line.to);
}
