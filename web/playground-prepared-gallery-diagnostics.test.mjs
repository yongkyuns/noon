import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../scripts/playground-prepared-lifecycle-qualification.mjs", import.meta.url), "utf8");
const start = source.indexOf("async function assertGallerySourceFailure(");
const end = source.indexOf("async function selectGalleryExample(", start);
assert.ok(start >= 0 && end > start, "the real browser qualification must expose its diagnostic check");
const check = vm.runInNewContext(`${source.slice(start, end)}\nassertGallerySourceFailure`, { assert });
const expectedTitle = "Python raised RuntimeError";
const expectedMessage = "intentional gallery source failure";

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

// Match the separate title/message/excerpt nodes of the production diagnostic
// panel. The excerpt deliberately contains the marker even for a wrong error.
function fixture({ summary = expectedTitle, title = expectedTitle, message = expectedMessage,
  delayed = null, failure = null } = {}) {
  const calls = [], reached = deferred(), gate = deferred();
  let visible = false;
  const step = async name => {
    calls.push(name);
    if (name === delayed) { reached.resolve(); await gate.promise; }
    if (name === failure?.step) throw failure.error;
    if (name === "visible") visible = true;
    else assert.equal(visible, true, "await the visible diagnostic before inspecting it");
  };
  const panel = {
    async waitFor(options) { assert.equal(options.state, "visible"); await step("visible"); },
    async textContent() { await step("whole-panel"); return `${title}\n${message}\nraise RuntimeError("${expectedMessage}")`; },
    locator(selector) {
      if (selector === "strong") return { async textContent() { await step("title"); return title; } };
      assert.equal(selector, "p", "read the diagnostic message, never the source excerpt");
      return { first: () => ({ async textContent() { await step("message"); return message; } }) };
    },
  };
  const page = { locator(selector) {
    if (selector === "#authoring-problem") return panel;
    assert.equal(selector, "#patch-status");
    return { async evaluate(callback) { await step("summary"); return callback({ value: summary }); } };
  } };
  return { page, calls, reached: reached.promise, release: gate.resolve };
}

test("gallery failure reads the visible diagnostic message separately from its exception title", async () => {
  const f = fixture();
  await check(f.page);
  assert.deepEqual(f.calls, ["visible", "summary", "title", "message"]);
});

for (const [name, values] of [
  ["wrong status title", { summary: "Python raised ValueError" }],
  ["wrong diagnostic title", { title: "Python raised ValueError" }],
  ["unrelated runtime error despite the same source excerpt", { message: "worker disconnected" }],
  ["missing exception message despite the same source excerpt", { message: "" }],
  ["marker only in the status", { summary: expectedMessage, message: "unrelated error" }],
]) {
  test(`gallery failure rejects ${name}`, async () => {
    await assert.rejects(check(fixture(values).page), { code: "ERR_ASSERTION" });
  });
}

for (const delayed of ["visible", "summary", "title", "message"]) {
  test(`gallery failure awaits ${delayed} before accepting the source error`, async () => {
    const f = fixture({ delayed });
    let settled = false;
    const pending = check(f.page);
    const observed = pending.then(() => { settled = true; });
    await f.reached;
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(settled, false);
    assert.equal(f.calls.at(-1), delayed);
    f.release(); await observed;
    assert.deepEqual(f.calls, ["visible", "summary", "title", "message"]);
  });
}

for (const step of ["visible", "summary", "title", "message"]) {
  test(`gallery failure propagates the original ${step} failure without retry`, async () => {
    const error = new Error(`original ${step} failure`);
    const f = fixture({ failure: { step, error } });
    await assert.rejects(check(f.page), actual => actual === error);
    assert.equal(f.calls.filter(name => name === step).length, 1);
    assert.equal(f.calls.at(-1), step);
  });
}

test("real gallery verifies the completed source error before qualifying partial-history pixels", () => {
  const call = source.indexOf("        await assertGallerySourceFailure(page);");
  const selected = source.lastIndexOf('await selectGalleryExample(page, "prepared-failure")', call);
  const settled = source.lastIndexOf('document.querySelector("#patch-status")?.dataset.state === "error"', call);
  const history = source.indexOf("        await openReady(page); await seekOpen(page, 5);", call);
  assert.ok(selected >= 0 && settled > selected && call > settled && history > call);
  assert.equal(source.match(/await assertGallerySourceFailure\(page\);/g)?.length, 1);
});
