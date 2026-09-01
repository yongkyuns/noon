#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { pathToFileURL } from "node:url";

const RENDERER_CRITICAL_PREFIXES = Object.freeze([
  "crates/noon-render-wgpu/",
  "crates/noon-text-render-wgpu/",
  "crates/noon-web/",
]);

const RENDERER_CRITICAL_FILES = new Set([
  "scripts/browser-smoke.mjs",
  "web/authoring-render-worker.js",
  "web/browser-smoke.html",
  "web/browser-smoke.js",
  "web/render-worker.js",
]);

export function classifyChangedPaths(paths) {
  const normalized = paths
    .map((path) => String(path).trim())
    .filter(Boolean);

  const rendererCriticalPaths = normalized.filter(
    (path) =>
      RENDERER_CRITICAL_FILES.has(path) ||
      RENDERER_CRITICAL_PREFIXES.some((prefix) => path.startsWith(prefix)),
  );

  return {
    rendererCritical: rendererCriticalPaths.length > 0,
    rendererCriticalPaths,
  };
}

export function changedPathsBetween(base, head) {
  if (!base || !head) {
    throw new Error("base and head refs are required");
  }
  const output = execFileSync(
    "git",
    ["diff", "--name-only", "--diff-filter=ACMRTUXB", `${base}...${head}`],
    { encoding: "utf8" },
  );
  return output.split("\n").filter(Boolean);
}

function writeGithubOutput(path, classification) {
  const lines = [
    `renderer_critical=${classification.rendererCritical ? "true" : "false"}`,
    `renderer_critical_paths=${classification.rendererCriticalPaths.join(",")}`,
  ];
  fs.appendFileSync(path, `${lines.join("\n")}\n`);
}

function main(argv) {
  const [base, head] = argv;
  const classification = classifyChangedPaths(changedPathsBetween(base, head));
  const outputPath = process.env.GITHUB_OUTPUT;
  if (outputPath) {
    writeGithubOutput(outputPath, classification);
  }
  process.stdout.write(`${JSON.stringify(classification)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2));
}
