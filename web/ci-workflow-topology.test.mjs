import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";

const workflowDir = new URL("../.github/workflows/", import.meta.url);
const workflowFiles = (await readdir(workflowDir))
  .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
  .sort();

assert.ok(workflowFiles.length > 0, "CI workflow inventory must not be empty");

const exactFamilies = new Map([
  ["pr-fast.yml", "pr-fast"],
  ["ci.yml", "main"],
  ["architecture-ratchets.yml", "architecture"],
  ["layer-dependency-ratchet.yml", "architecture"],
  ["compiler-cache-seed.yml", "cache-seed"],
  ["test-coverage.yml", "coverage"],
  ["platform-release.yml", "platform-release"],
  ["native-host-smoke.yml", "native-host"],
  ["pages.yml", "deployment"],
  ["fuzz.yml", "fuzz"],
  ["branch-cleanup-once.yml", "maintenance"],
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
  "pr-fast",
  "main",
  "architecture",
  "cache-seed",
  "coverage",
  "platform-release",
  "manim",
  "playground",
]);
const presentFamilies = new Set(classified.map(({ family }) => family));
for (const family of requiredFamilies) {
  assert.ok(presentFamilies.has(family), `required CI family ${family} must remain represented`);
}

const pagesWorkflow = await readFile(new URL("pages.yml", workflowDir), "utf8");
await readFile(new URL(".nojekyll", import.meta.url));
const uploadPagesStep = pagesWorkflow
  .split("\n      - name: Upload Pages artifact\n")[1]
  ?.split("\n      - ")[0];
assert.ok(uploadPagesStep, "Pages workflow must retain its artifact upload step");
assert.match(
  uploadPagesStep,
  /^\s*include-hidden-files:\s*true$/m,
  "Pages upload must include web/.nojekyll so GitHub Pages serves built assets directly",
);

const counts = Object.fromEntries(
  [...presentFamilies].sort().map((family) => [
    family,
    classified.filter((entry) => entry.family === family).length,
  ]),
);
console.log(`✓ classified ${workflowFiles.length} CI workflows: ${JSON.stringify(counts)}`);
