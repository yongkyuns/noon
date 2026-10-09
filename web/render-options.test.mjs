import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { resolveRenderOptionsPlain } from "./render-options.js";

test("browser projection forwards every option unchanged and releases its Rust result", () => {
  let calls = 0, freed = 0;
  const expected = { pixelWidth: 322, pixelHeight: 182, frameRateNumerator: 123,
    frameRateDenominator: 7, format: "png" };
  const args = ["l", "65,33", "60000/1001", "png", undefined, undefined];
  const result = resolveRenderOptionsPlain((...actual) => {
    calls++;
    assert.deepEqual(actual, args);
    return { ...expected, free() { freed++; } };
  }, ...args);
  assert.deepEqual(result, expected);
  assert.equal(calls, 1);
  assert.equal(freed, 1);
});

test("projection preserves resolver failures and still releases after a getter error", () => {
  const error = new Error("bad Rust configuration");
  assert.throws(() => resolveRenderOptionsPlain(() => { throw error; }), e => e === error);
  let freed = false;
  assert.throws(() => resolveRenderOptionsPlain(() => ({
    get pixelWidth() { throw error; }, free() { freed = true; },
  })), e => e === error);
  assert.ok(freed);
});

test("CPython and WASM delegates share the Rust resolver, worker only projects values", () => {
  for (const file of ["crates/noon-python/src/render_options.rs", "crates/noon-web/src/render_options.rs"]) {
    const source = readFileSync(file, "utf8");
    assert.match(source, /RenderOptionInputs/);
    assert.match(source, /\.resolve\(\)/);
    assert.doesNotMatch(source, /854|1280|1920|3840|60000|\.parse/);
  }
  const worker = readFileSync("web/python-worker.source.js", "utf8");
  assert.match(worker, /noonResolveRenderOptions = .*resolveRenderOptionsPlain\(resolveRenderOptions/);
  assert.match(readFileSync("web/python-compat-modules.js", "utf8"), /_noon_render_options\.py/);
});
