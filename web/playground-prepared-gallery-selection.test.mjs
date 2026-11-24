import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const source = await readFile(new URL("../scripts/playground-prepared-lifecycle-qualification.mjs", import.meta.url), "utf8");
const start = source.indexOf("async function selectGalleryExample(");
const end = source.indexOf("async function pixels(", start);
assert.ok(start >= 0 && end > start);
const select = vm.runInNewContext(source.slice(start, end) + "\nselectGalleryExample;");
const stages = ["trigger", "visible", "card", "hidden"];
function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

// Qualify the actual harness helper, not the renderer or gallery implementation.
// Delayed DOM action doubles reject hidden/forced clicks and unawaited steps;
// the real-browser matrix remains the end-to-end evidence.
function fixture({ blocked = null, failureAt = null } = {}) {
  const events = [], selected = [], entered = Object.fromEntries(stages.map(stage => [stage, deferred()]));
  const gate = deferred(), failure = new Error("DOM action failed");
  let open = false, clicked = false;
  async function step(stage) {
    events.push(stage);
    entered[stage].resolve();
    if (failureAt === stage) throw failure;
    if (blocked === stage) await gate.promise;
  }
  const layer = {
    async waitFor({ state }) {
      assert.ok(state === "visible" || state === "hidden");
      assert.equal(open, true, "picker must have opened through its trigger");
      if (state === "hidden") assert.equal(clicked, true, "selection must precede closure");
      await step(state);
      if (state === "hidden") open = false;
    },
    locator(selector) {
      const match = /^\.example-card\[data-example-id="([a-z-]+)"\]$/.exec(selector);
      assert.ok(match, "select the requested card inside the picker");
      return { async click(options) {
        assert.equal(options, undefined, "do not bypass DOM actionability with force");
        assert.equal(open, true, "a hidden card cannot be clicked");
        await step("card");
        selected.push(match[1]); clicked = true;
      } };
    },
  };
  const page = { locator(selector) {
    if (selector === ".example-browser-layer") return layer;
    assert.equal(selector, "#example-browser-trigger");
    return { async click(options) {
      assert.equal(options, undefined);
      assert.equal(open, false, "each selection starts with the picker closed");
      await step("trigger"); open = true; clicked = false;
    } };
  } };
  return { page, events, selected, entered, gate, failure, get open() { return open; } };
}

test("both prepared-gallery transitions use the visible picker helper", () => {
  for (const id of ["prepared-failure", "prepared-lifecycle"]) {
    assert.equal(source.split(`await selectGalleryExample(page, "${id}");`).length - 1, 1);
  }
  assert.doesNotMatch(source, /await page\.locator\('\.example-card\[data-example-id="[^"]+"\]'\)\.click\(/);
});

test("selection opens the picker, clicks its requested card and awaits closure", async () => {
  const f = fixture();
  await select(f.page, "prepared-failure");
  assert.deepEqual(f.events, stages);
  assert.deepEqual(f.selected, ["prepared-failure"]);
  assert.equal(f.open, false);
});

test("the recovery selection reopens the picker rather than clicking a hidden card", async () => {
  const f = fixture();
  await select(f.page, "prepared-failure");
  await select(f.page, "prepared-lifecycle");
  assert.deepEqual(f.events, [...stages, ...stages]);
  assert.deepEqual(f.selected, ["prepared-failure", "prepared-lifecycle"]);
  assert.equal(f.open, false);
});

for (const blocked of stages) {
  test(`selection awaits delayed ${blocked} before advancing`, async () => {
    const f = fixture({ blocked });
    let settled = false;
    const task = select(f.page, "prepared-failure").then(() => { settled = true; });
    await f.entered[blocked].promise;
    assert.deepEqual(f.events, stages.slice(0, stages.indexOf(blocked) + 1));
    assert.equal(settled, false);
    f.gate.resolve();
    await task;
    assert.deepEqual(f.events, stages);
    assert.deepEqual(f.selected, ["prepared-failure"]);
    assert.equal(f.open, false);
  });
}

for (const failureAt of stages) {
  test(`failed ${failureAt} propagates its original error without bypass or retry`, async () => {
    const f = fixture({ failureAt });
    await assert.rejects(select(f.page, "prepared-failure"), error => error === f.failure);
    assert.deepEqual(f.events, stages.slice(0, stages.indexOf(failureAt) + 1));
  });
}
