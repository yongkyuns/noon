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

const renderModeSwitchPaths = new Set([
  "scripts/authoring-render-mode-switch-smoke.mjs",
  // Policy changes must exercise the oracle they decide whether to run.
  "scripts/pr-risk-classifier.mjs",
  "web/authoring-execution-client.js",
  "web/authoring-render-worker.js",
  "web/execution-engine-worker.js",
  "web/execution-render-worker.js",
  "web/execution-transport.js",
  "web/execution-worker-client.js",
  "web/execution-worker-smoke.html",
  "web/retained-execution-engine-worker.js",
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

export function requiresRenderModeSwitchSmoke(path) {
  const normalized = normalizeRepositoryPath(path);
  return normalized !== "" && renderModeSwitchPaths.has(normalized);
}

export function classifyPrRisk(paths) {
  const normalizedPaths = [...new Set(paths.map(normalizeRepositoryPath).filter(Boolean))];
  const retainedExecutionPaths = normalizedPaths.filter(requiresRetainedExecutionSmoke).sort();
  const renderModeSwitchPaths = normalizedPaths.filter(requiresRenderModeSwitchSmoke).sort();

  return Object.freeze({
    retainedExecution: retainedExecutionPaths.length > 0,
    retainedExecutionPaths: Object.freeze(retainedExecutionPaths),
    renderModeSwitch: renderModeSwitchPaths.length > 0,
    renderModeSwitchPaths: Object.freeze(renderModeSwitchPaths),
  });
}

function runCli() {
  const paths = readFileSync(0, "utf8").split(/\r?\n/);
  const risk = classifyPrRisk(paths);
  process.stdout.write(`retained_execution=${risk.retainedExecution}\n`);
  process.stdout.write(`render_mode_switch=${risk.renderModeSwitch}\n`);

  const retainedDetail = risk.retainedExecution
    ? risk.retainedExecutionPaths.join(", ")
    : "none";
  const modeSwitchDetail = risk.renderModeSwitch
    ? risk.renderModeSwitchPaths.join(", ")
    : "none";
  process.stderr.write(`retained execution risk paths: ${retainedDetail}\n`);
  process.stderr.write(`render mode-switch risk paths: ${modeSwitchDetail}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runCli();
}
