import assert from "node:assert/strict";
import test from "node:test";

import { drainRendererGpuDiagnostics, formatGpuDiagnostic } from "./render-gpu-diagnostics.js";

function fakeRenderer(records) {
  const queue = [...records];
  return { takeGpuDiagnosticJson: () => queue.shift() ?? null };
}

function encoded(kind, severity, message = "diagnostic") {
  return JSON.stringify({ generation: 3, backend: "WebGPU", kind, severity, message });
}

test("recoverable validation drains without stopping", () => {
  const recoverable = [];
  const fatal = [];
  assert.equal(
    drainRendererGpuDiagnostics(
      fakeRenderer([encoded("validation", "recoverable", "invalid usage")]),
      {
        onRecoverable: (value) => recoverable.push(value),
        onFatal: (value) => fatal.push(value),
      },
    ),
    true,
  );
  assert.equal(recoverable.length, 1);
  assert.deepEqual(fatal, []);
});

test("fatal diagnostics stop draining", () => {
  const recoverable = [];
  const fatal = [];
  assert.equal(
    drainRendererGpuDiagnostics(
      fakeRenderer([
        encoded("out_of_memory", "fatal", "oom"),
        encoded("validation", "recoverable", "must not drain"),
      ]),
      {
        onRecoverable: (value) => recoverable.push(value),
        onFatal: (value) => fatal.push(value),
      },
    ),
    false,
  );
  assert.deepEqual(recoverable, []);
  assert.equal(fatal.length, 1);
});

test("rejects inconsistent severity", () => {
  assert.throws(
    () =>
      drainRendererGpuDiagnostics(fakeRenderer([encoded("validation", "fatal")]), {
        onRecoverable() {},
        onFatal() {},
      }),
    /validation diagnostics must be recoverable/,
  );
});

test("formats diagnostic context", () => {
  assert.equal(
    formatGpuDiagnostic({
      generation: 5,
      backend: "WebGPU",
      kind: "validation",
      message: "invalid buffer",
    }),
    "WebGPU generation 5 validation: invalid buffer",
  );
});
