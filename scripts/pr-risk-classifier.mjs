import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const retainedExecutionWebPaths = new Set([
  "scripts/retained-execution-worker-smoke.mjs",
  "web/authoring-execution-client.js",
  "web/authoring-render-worker.js",
  "web/execution-canvas.js",
  "web/execution-render-worker.js",
  "web/execution-transport.js",
  "web/execution-worker-client.js",
  "web/execution-worker-smoke.html",
  "web/render-gpu-diagnostics.js",
  "web/retained-execution-engine-worker.js",
  "web/scene-identity.js",
]);

const retainedExecutionRustPaths = new Set([
  // This module wires the retained WASM exports consumed by the browser workers.
  "crates/noon-web/src/lib.rs",
]);

function normalizeRepositoryPath(path) {
  return path.trim().replaceAll("\\", "/").replace(/^\.\//, "");
}

export function requiresRetainedExecutionSmoke(path) {
  const normalized = normalizeRepositoryPath(path);
  if (normalized === "") return false;
  if (retainedExecutionWebPaths.has(normalized)) return true;
  if (retainedExecutionRustPaths.has(normalized)) return true;

  // Retained execution is deliberately named at its Rust ownership boundaries.
  // Escalate any retained-specific crate source/test change without forcing generic
  // geometry, renderer, authoring, or demo edits through the worker smoke.
  return normalized.startsWith("crates/") && /(^|\/)[^/]*retained[^/]*(\/|$)/.test(normalized);
}

export function classifyPrRisk(paths) {
  const retainedExecutionPaths = [...new Set(
    paths
      .map(normalizeRepositoryPath)
      .filter(Boolean)
      .filter(requiresRetainedExecutionSmoke),
  )].sort();

  return Object.freeze({
    retainedExecution: retainedExecutionPaths.length > 0,
    retainedExecutionPaths: Object.freeze(retainedExecutionPaths),
  });
}

function runCli() {
  const paths = readFileSync(0, "utf8").split(/\r?\n/);
  const risk = classifyPrRisk(paths);
  process.stdout.write(`retained_execution=${risk.retainedExecution}\n`);

  const detail = risk.retainedExecution
    ? risk.retainedExecutionPaths.join(", ")
    : "none";
  process.stderr.write(`retained execution risk paths: ${detail}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runCli();
}
