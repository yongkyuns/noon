import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const retainedRenderer = await readFile(
  new URL("../crates/noon-web/src/retained_execution_canvas.rs", import.meta.url),
  "utf8",
);

const flushStart = retainedRenderer.indexOf(
  "#[wasm_bindgen(js_name = flushGpuDiagnostics)]",
);
const flushEnd = retainedRenderer.indexOf(
  "#[wasm_bindgen(js_name = takeGpuDiagnosticJson)]",
  flushStart,
);
assert.ok(flushStart >= 0 && flushEnd > flushStart, "GPU diagnostic flush export must exist");
const flushSource = retainedRenderer.slice(flushStart, flushEnd);

test("WebGPU diagnostic flush does not hold a mutable wasm renderer borrow across await", () => {
  assert.match(
    flushSource,
    /pub fn flush_gpu_diagnostics\(&mut self\) -> js_sys::Promise/,
  );
  assert.doesNotMatch(
    flushSource,
    /pub async fn flush_gpu_diagnostics\(&mut self\)/,
    "an async wasm-bindgen &mut self export would keep the renderer borrowed while WebGPU awaits pop_error_scope",
  );
  assert.match(flushSource, /let pending = scope\.pop\(\);/);
  assert.match(flushSource, /future_to_promise\(async move/);
});

test("WebGPU diagnostic flush re-arms synchronously before detaching validation delivery", () => {
  const popIndex = flushSource.indexOf("let pending = scope.pop();");
  const rearmIndex = flushSource.indexOf("self.device.push_error_scope");
  const detachIndex = flushSource.indexOf("future_to_promise(async move");
  assert.ok(popIndex >= 0 && rearmIndex > popIndex && detachIndex > rearmIndex);
  for (const captured of [
    "let gpu_diagnostics = self.gpu_diagnostics.clone();",
    "let gpu_generation = self.gpu_generation;",
    "let backend = self.backend;",
  ]) {
    const captureIndex = flushSource.indexOf(captured);
    assert.ok(
      captureIndex > rearmIndex && captureIndex < detachIndex,
      `detached validation delivery must capture ${captured}`,
    );
  }
});
