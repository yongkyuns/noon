import { createHash } from "node:crypto";

// One valid ready-message fixture for the verifier and authoring boundary tests.
export function runtimeBuildIdentity() {
  const core = {
    schema: 1,
    sourceRevision: "a".repeat(40),
    files: {
      worker: { path: "./python-worker.js", sha256: "b".repeat(64) },
      wasm: { path: "./pkg/noon_web_bg.wasm", sha256: "c".repeat(64) },
      glue: { path: "./pkg/noon_web.js", sha256: "d".repeat(64) },
      verifier: { path: "./runtime-build-verifier.js", sha256: "e".repeat(64) },
    },
  };
  return { ...core, buildId: createHash("sha256").update(JSON.stringify(core)).digest("hex") };
}
