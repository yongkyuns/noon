import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const retainedExecutionWebPaths = new Set([
  ".github/workflows/pr-fast.yml",
  "scripts/pr-risk-classifier.mjs",
  "scripts/browser-test-server.mjs",
  "web/authoring-client.js",
  "web/python-worker.source.js",
  "web/semantic-engine-endpoint.js",
  "scripts/execution-worker-smoke.mjs",
  "web/authoring-execution-client.js",
  "web/authoring-render-worker.js",
  "web/execution-canvas.js",
  "web/execution-render-worker.js",
  "web/execution-transport.js",
  "web/execution-worker-client.js",
  "web/execution-worker-smoke.html",
  "web/render-gpu-diagnostics.js",
]);

const retainedExecutionRustPaths = new Set([
  // Shared runtime publication and its WASM exports feed the retained renderer.
  "crates/noon-web/src/lib.rs",
  "crates/noon-web/src/semantic_execution_player.rs",
]);

const hostUpdaterDiagnosticPaths = new Set([
  ".github/workflows/pr-fast.yml",
  "crates/noon-render-wgpu/src/gpu/retained_text.rs",
  "crates/noon-render-wgpu/src/text/gpu.rs",
  "crates/noon-render-wgpu/src/text/preparation.rs",
  "crates/noon-web/src/renderer_observation.rs",
  "crates/noon-web/src/renderer_observation/retained.rs",
  "crates/noon-web/src/retained_execution_canvas.rs",
  "crates/noon-web/src/semantic_execution_player.rs",
  "scripts/manim-host-updater-diagnostics.mjs",
  "web/authoring-execution-client.js",
  "web/authoring-render-worker.js",
  "web/execution-worker-client.js",
  "web/python/examples/renderer_observation_callbacks.py",
  "web/semantic-engine-endpoint.js",
]);

const rendererCriticalPrefixes = Object.freeze([
  "crates/noon-render-wgpu/",
  "crates/noon-web/",
]);

const rendererCriticalFiles = new Set([
  ".github/workflows/pr-fast.yml",
  "scripts/browser-smoke.mjs",
  // Classifier policy changes must exercise the WebGL oracle they decide whether to run.
  "scripts/pr-risk-classifier.mjs",
  "web/authoring-render-worker.js",
  "web/browser-smoke.html",
  "web/browser-smoke.js",
  "web/execution-render-worker.js",
  "web/render-gpu-diagnostics.js",
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

export function requiresHostUpdaterDiagnostic(path) {
  const normalized = normalizeRepositoryPath(path);
  return hostUpdaterDiagnosticPaths.has(normalized);
}

export function requiresRendererCriticalWebglSmoke(path) {
  const normalized = normalizeRepositoryPath(path);
  if (normalized === "") return false;
  if (rendererCriticalFiles.has(normalized)) return true;
  return rendererCriticalPrefixes.some((prefix) => normalized.startsWith(prefix));
}

export function classifyPrRisk(paths) {
  const normalizedPaths = [...new Set(paths.map(normalizeRepositoryPath).filter(Boolean))];
  const retainedExecutionPaths = normalizedPaths.filter(requiresRetainedExecutionSmoke).sort();
  const hostUpdaterDiagnosticPathsChanged = normalizedPaths.filter(requiresHostUpdaterDiagnostic).sort();
  const rendererCriticalPaths = normalizedPaths.filter(requiresRendererCriticalWebglSmoke).sort();

  return Object.freeze({
    retainedExecution: retainedExecutionPaths.length > 0,
    retainedExecutionPaths: Object.freeze(retainedExecutionPaths),
    hostUpdaterDiagnostics: hostUpdaterDiagnosticPathsChanged.length > 0,
    hostUpdaterDiagnosticPaths: Object.freeze(hostUpdaterDiagnosticPathsChanged),
    rendererCritical: rendererCriticalPaths.length > 0,
    rendererCriticalPaths: Object.freeze(rendererCriticalPaths),
  });
}

function runCli() {
  const paths = readFileSync(0, "utf8").split(/\r?\n/);
  const risk = classifyPrRisk(paths);
  process.stdout.write(`retained_execution=${risk.retainedExecution}\n`);
  process.stdout.write(`host_updater_diagnostics=${risk.hostUpdaterDiagnostics}\n`);
  process.stdout.write(`renderer_critical=${risk.rendererCritical}\n`);
  process.stdout.write(`renderer_critical_paths=${risk.rendererCriticalPaths.join(",")}\n`);

  const retainedDetail = risk.retainedExecution
    ? risk.retainedExecutionPaths.join(", ")
    : "none";
  const hostUpdaterDetail = risk.hostUpdaterDiagnostics
    ? risk.hostUpdaterDiagnosticPaths.join(", ")
    : "none";
  const rendererDetail = risk.rendererCritical
    ? risk.rendererCriticalPaths.join(", ")
    : "none";
  process.stderr.write(`retained execution risk paths: ${retainedDetail}\n`);
  process.stderr.write(`host-updater diagnostic risk paths: ${hostUpdaterDetail}\n`);
  process.stderr.write(`renderer-critical WebGL risk paths: ${rendererDetail}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runCli();
}
