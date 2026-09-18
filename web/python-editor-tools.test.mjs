import assert from "node:assert/strict";
import test from "node:test";
import { sourceEdit, formatEditorSource } from "./python-editor-tools.js";

test("localized formatter edits reproduce the formatted source including unicode and deletions", () => {
  for (const [before, after] of [["x=1\n", "x = 1\n"], ["alpha", "alp"], ["", "x\n"], ["identical", "identical"], ["# 한글😀\nx=1", "# 한글😀\nx = 1\n"]]) {
    const { from, to, insert } = sourceEdit(before, after);
    assert.equal(before.slice(0, from) + insert + before.slice(to), after);
  }
});

function editor() {
  const doc = {};
  const writes = [];
  const textarea = { value: "x=1\n", editorView: { state: { doc }, dispatch: (change) => writes.push(change) } };
  return { textarea, writes, doc };
}

test("formatting uses one normal undoable editor transaction", async () => {
  const { textarea, writes } = editor();
  assert.equal(await formatEditorSource(textarea, async () => "x = 1\n"), "Formatted with Ruff");
  assert.equal(writes.length, 1);
  assert.equal(writes[0].userEvent, "input.format");
});

test("async formatting never overwrites a newer or programmatically selected document", async () => {
  const { textarea, writes } = editor();
  const result = await formatEditorSource(textarea, async () => {
    textarea.editorView.state = { doc: {} };
    return "x = 1\n";
  });
  assert.equal(result, "Source changed; format again");
  assert.deepEqual(writes, []);
});

test("lint and cursor-only transactions do not invalidate formatting", async () => {
  const { textarea, writes, doc } = editor();
  await formatEditorSource(textarea, async () => {
    textarea.editorView.state = { doc, selection: "new cursor" };
    return "x = 1\n";
  });
  assert.equal(writes.length, 1);
});

test("native fallback formatting emits the same source input contract", async () => {
  const events = [];
  const textarea = {
    value: "x=1", dispatchEvent: (event) => events.push(event.type),
    setRangeText(insert, from, to) { this.value = this.value.slice(0, from) + insert + this.value.slice(to); },
  };
  await formatEditorSource(textarea, async () => "x = 1\n");
  assert.equal(textarea.value, "x = 1\n");
  assert.deepEqual(events, ["input"]);
});
