import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";

const workflowDir = new URL("../.github/workflows/", import.meta.url);
const workflowFiles = (await readdir(workflowDir))
  .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
  .sort();

assert.ok(workflowFiles.length > 0, "CI workflow inventory must not be empty");

const exactFamilies = new Map([
  ["ci.yml", "main"],
  ["test-coverage.yml", "coverage"],
  ["platform-release.yml", "platform-release"],
  ["pages.yml", "deployment"],
  ["fuzz.yml", "fuzz"],
]);

function classifyWorkflow(name) {
  if (exactFamilies.has(name)) return exactFamilies.get(name);
  for (const [prefix, family] of [
    ["manim-", "manim"],
    ["playground-", "playground"],
    ["renderer-", "renderer"],
    ["authoring-", "authoring"],
    ["retained-", "retained"],
    ["perf-", "performance"],
  ]) {
    if (name.startsWith(prefix)) return family;
  }
  return null;
}

const classified = workflowFiles.map((name) => ({ name, family: classifyWorkflow(name) }));
const unclassified = classified.filter(({ family }) => family === null).map(({ name }) => name);
assert.deepEqual(
  unclassified,
  [],
  `new workflow families must be classified explicitly: ${unclassified.join(", ")}`,
);

const requiredFamilies = new Set([
  "main",
  "coverage",
  "platform-release",
  "manim",
  "playground",
]);
const presentFamilies = new Set(classified.map(({ family }) => family));
for (const family of requiredFamilies) {
  assert.ok(presentFamilies.has(family), `required CI family ${family} must remain represented`);
}

const ciDocs = await readFile(new URL("../docs/ci.md", import.meta.url), "utf8");
for (const [needle, purpose] of [
  [".github/workflows/ci.yml", "main CI"],
  [".github/workflows/platform-release.yml", "platform/release validation"],
  [".github/workflows/test-coverage.yml", "test observability"],
  ["Manim compatibility workflows", "Manim validation family"],
]) {
  assert.match(ciDocs, new RegExp(needle.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")), `${purpose} must remain documented`);
}

const counts = Object.fromEntries(
  [...presentFamilies].sort().map((family) => [
    family,
    classified.filter((entry) => entry.family === family).length,
  ]),
);
console.log(`✓ classified ${workflowFiles.length} CI workflows: ${JSON.stringify(counts)}`);
