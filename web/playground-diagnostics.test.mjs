import assert from "node:assert/strict";
import test from "node:test";
import { PlaygroundDiagnostics } from "./playground-diagnostics.js";

class Element {
  children = []; listeners = new Map(); hidden = false; open = false; textContent = "";
  attributes = {}; value = "missing"; focused = false;
  ownerDocument = { createElement: () => new Element() };
  append(...children) { this.children.push(...children); }
  before(node) { this.previous = node; }
  querySelector() { return new Element(); }
  setAttribute(key, value) { this.attributes[key] = value; }
  addEventListener(name, callback) { this.listeners.set(name, callback); }
  removeEventListener(name) { this.listeners.delete(name); }
  focus() { this.focused = true; }
  setSelectionRange(from, to) { this.selection = { from, to }; }
  remove() { this.removed = true; }
  emit(name) { this.listeners.get(name)?.(); }
}
function error() {
  const failure = new Error("raw stack information");
  failure.authoringSource = "missing";
  failure.authoringDiagnostic = { version: 1, phase: "construct", truncated: false,
    document: { filename: "scene.py", revision: "run-1" },
    exception: { type: "NameError", module: "builtins", message: "<img src=x onerror=alert(1)>",
      location: { line: 1, column: 0, endLine: 1, endColumn: 7 }, frames: [],
      notes: [], engine: null, cause: null, causeRelation: null, children: [] } };
  return failure;
}
function setup() {
  const editor = new Element(), details = new Element(), traceback = new Element();
  const view = new PlaygroundDiagnostics(editor, details, traceback);
  return { editor, details, traceback, view, panel: details.previous, button: details.previous.children.at(-1) };
}
test("structured error uses literal message, source link and closed technical details", () => {
  const { view, details, traceback, panel, button } = setup();
  assert.equal(view.show(error()), "Python raised NameError");
  assert.equal(panel.hidden, false);
  assert.equal(panel.children[1].textContent, "<img src=x onerror=alert(1)>");
  assert.equal(button.textContent, "Go to scene.py:1");
  assert.equal(button.hidden, false);
  assert.equal(details.open, false);
  assert.match(traceback.textContent, /raw stack information/);
});
test("textarea fallback selects exact failing source", () => {
  const { view, editor, button } = setup();
  view.show(error()); button.emit("click");
  assert.equal(editor.focused, true);
  assert.deepEqual(editor.selection, { from: 0, to: 7 });
});
test("CodeMirror navigation uses its committed document selection", () => {
  const { view, editor, button } = setup();
  const calls = [];
  editor.editorView = { dispatch: (value) => calls.push(value), focus: () => calls.push("focus") };
  view.show(error()); button.emit("click");
  assert.deepEqual(calls, [{ selection: { anchor: 0, head: 7 }, scrollIntoView: true }, "focus"]);
});
test("real input clears stale diagnostics even if text is later restored", () => {
  const { view, editor, panel, button, traceback } = setup();
  view.show(error()); editor.emit("input");
  editor.value = "different"; editor.value = "missing";
  button.emit("click");
  assert.equal(panel.hidden, true); assert.equal(editor.selection, undefined);
  assert.equal(traceback.textContent, "");
});
test("programmatic text changes cannot navigate to an unrelated range", () => {
  const { view, editor, button } = setup();
  view.show(error()); editor.value = "another source"; button.emit("click");
  assert.equal(editor.selection, undefined);
});
test("mismatched source and unknown failures never manufacture links", () => {
  const { view, editor, button, details } = setup();
  editor.value = "another source"; view.show(error()); assert.equal(button.hidden, true);
  view.show(new Error("plain failure"), "Preview unavailable");
  assert.equal(button.hidden, true); assert.equal(details.open, false);
});
test("a new error resets manually expanded details", () => {
  const { view, details } = setup();
  view.show(error()); details.open = true; view.show(error());
  assert.equal(details.open, false);
});
test("dispose releases source and listeners", () => {
  const { view, editor, panel, traceback, button } = setup();
  view.show(error()); view.dispose(); button.emit("click");
  assert.equal(editor.listeners.size, 0); assert.equal(panel.removed, true);
  assert.equal(traceback.textContent, ""); assert.equal(editor.selection, undefined);
});
