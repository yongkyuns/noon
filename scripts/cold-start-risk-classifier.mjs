import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const coldStartBenchmarkPaths = new Set([
  ".github/workflows/pr-fast.yml",
  "scripts/build-web-demo.sh",
  "scripts/cold-start-risk-classifier.mjs",
  "scripts/cold-start-risk-classifier.test.mjs",
  "scripts/playground-cold-start.mjs",
  "web/live-authoring-bootstrap.js",
  "web/main.js",
  "web/playground-cold-start-metrics.js",
  "web/playground-cold-start-metrics.test.mjs",
  "web/python-worker.source.js",
]);

function normalizeRepositoryPath(path) {
  return String(path ?? "").trim().replaceAll("\\", "/").replace(/^\.\//, "");
}

export function requiresColdStartBenchmark(path) {
  return coldStartBenchmarkPaths.has(normalizeRepositoryPath(path));
}

export function classifyColdStartRisk(paths) {
  if (!Array.isArray(paths)) {
    throw new TypeError("cold-start risk paths must be an array");
  }
  const normalizedPaths = [...new Set(paths.map(normalizeRepositoryPath).filter(Boolean))];
  const benchmarkPaths = normalizedPaths.filter(requiresColdStartBenchmark).sort();
  return Object.freeze({
    coldStartBenchmark: benchmarkPaths.length > 0,
    coldStartBenchmarkPaths: Object.freeze(benchmarkPaths),
  });
}

function runCli() {
  const paths = readFileSync(0, "utf8").split(/\r?\n/);
  const risk = classifyColdStartRisk(paths);
  process.stdout.write(`cold_start=${risk.coldStartBenchmark}\n`);
  const detail = risk.coldStartBenchmark ? risk.coldStartBenchmarkPaths.join(", ") : "none";
  process.stderr.write(`cold-start benchmark risk paths: ${detail}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runCli();
}
