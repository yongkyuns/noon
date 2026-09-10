import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const workflowDir = new URL("../.github/workflows/", import.meta.url);
const workflowFiles = (await readdir(workflowDir))
  .filter((name) => name.endsWith(".yml") || name.endsWith(".yaml"))
  .sort();

assert.ok(workflowFiles.length > 0, "CI workflow inventory must not be empty");

const exactFamilies = new Map([
  ["pr-fast.yml", "pr-fast"],
  ["ci.yml", "main"],
  ["ci-measurements.yml", "ci-diagnostics"],
  ["architecture-ratchets.yml", "architecture"],
  ["architecture-diagrams.yml", "architecture"],
  ["provider-features.yml", "provider-isolation"],
  ["layer-dependency-ratchet.yml", "architecture"],
  ["compiler-cache-seed.yml", "cache-seed"],
  ["test-coverage.yml", "coverage"],
  ["platform-release.yml", "platform-release"],
  ["native-host-smoke.yml", "native-host"],
  ["pages.yml", "deployment"],
  ["fuzz.yml", "fuzz"],
  ["branch-cleanup-once.yml", "maintenance"],
  ["noon-agent-foundation.yml", "agent-authoring"],
  ["agent-preview-isolation.yml", "agent-preview"],
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
  "provider-isolation",
  "cache-seed",
  "coverage",
  "platform-release",
  "manim",
  "playground",
  "agent-authoring",
  "agent-preview",
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


// Read the actual event filter. Deliberately accept only this workflow's simple
// quoted path list: new event constraints or glob syntax must extend these tests,
// not silently receive different semantics from GitHub's selector.
function providerPullRequestPaths(workflow) {
  const lines = workflow.split("\n").filter((line) => line.trim() && !line.trimStart().startsWith("#"));
  const start = lines.indexOf("on:");
  assert.ok(start >= 0, "provider workflow must declare on:");
  const end = lines.findIndex((line, index) => index > start && !line.startsWith(" "));
  const events = lines.slice(start + 1, end < 0 ? undefined : end);
  const pr = events.indexOf("  pull_request:");
  assert.ok(pr >= 0, "provider workflow must select pull requests");
  const next = events.findIndex((line, index) => index > pr && /^  \S/.test(line));
  const filter = events.slice(pr + 1, next < 0 ? undefined : next);
  assert.equal(filter.shift(), "    paths:", "provider PR selection must be paths-only");
  assert.ok(filter.length > 0, "provider path filter must not be empty");
  return filter.map((line) => {
    const match = /^      - (['"])([A-Za-z0-9_./*-]+)\1(?:\s+#.*)?$/.exec(line);
    assert.ok(match, `unsupported provider path filter: ${line}`);
    return match[2];
  });
}

function providerPathMatches(pattern, changedPath) {
  // GitHub's literal paths and whole-segment **, including zero directories.
  // Reject unsupported syntax instead of approximating ?, [], negation, or YAML.
  const parts = pattern.split("/");
  const expression = parts.map((part, index) => {
    const last = index === parts.length - 1;
    if (part === "**") return last ? ".*" : "(?:[^/]+/)*";
    assert.match(part, /^[A-Za-z0-9_.-]+$/, `unsupported provider glob: ${pattern}`);
    return part.replaceAll(".", "\\.") + (last ? "" : "/");
  }).join("");
  return new RegExp(`^${expression}$`).test(changedPath);
}

const providerWorkflow = await readFile(new URL("provider-features.yml", workflowDir), "utf8");
const providerPaths = providerPullRequestPaths(providerWorkflow);
const selectsProvider = (paths) => paths.some((changed) =>
  providerPaths.some((pattern) => providerPathMatches(pattern, changed)));

test("provider qualification selects each consumer input even when changed alone", () => {
  for (const changed of [
    "crates/noon/examples/provider_probe.rs",
    "crates/noon/examples/nested/support.rs",
    "crates/noon/src/lib.rs",
    "crates/noon/tests/public_api.rs",
    "crates/noon/benches/authoring.rs",
    "crates/noon/build.rs",
    "crates/noon/Cargo.toml",
    "crates/noon-core/src/lib.rs",
    "crates/noon-core/tests/property_invariants.rs",
    "crates/noon-core/benches/store.rs",
    "crates/noon-core/build.rs",
    "crates/noon-core/Cargo.toml",
    "crates/noon-compile/src/lib.rs",
    "crates/noon-compile/tests/analytic_transform.rs",
    "crates/noon-compile/benches/lowering.rs",
    "crates/noon-compile/build.rs",
    "crates/noon-compile/Cargo.toml",
    "crates/noon-runtime/src/lib.rs",
    "crates/noon-runtime/tests/static_frame_locality.rs",
    "crates/noon-runtime/benches/runtime.rs",
    "crates/noon-runtime/build.rs",
    "crates/noon-runtime/Cargo.toml",
    "crates/noon-render-wgpu/src/lib.rs",
    "crates/noon-render-wgpu/tests/appearance.rs",
    "crates/noon-render-wgpu/benches/renderer.rs",
    "crates/noon-render-wgpu/build.rs",
    "crates/noon-render-wgpu/Cargo.toml",
    "crates/noon-native/src/lib.rs",
    "crates/noon-native/examples/create_shapes.rs",
    "crates/noon-native/tests/smoke.rs",
    "crates/noon-native/benches/native.rs",
    "crates/noon-native/build.rs",
    "crates/noon-native/Cargo.toml",
    "crates/noon-web/src/lib.rs",
    "crates/noon-web/tests/deterministic_replay.rs",
    "crates/noon-web/benches/web.rs",
    "crates/noon-web/build.rs",
    "crates/noon-web/Cargo.toml",
    "crates/noon-text/src/lib.rs",
    "crates/noon-text/tests/metrics.rs",
    "crates/noon-text/benches/text.rs",
    "crates/noon-text/build.rs",
    "crates/noon-text/Cargo.toml",
    "crates/noon-typst/src/lib.rs",
    "crates/noon-typst/tests/smoke.rs",
    "crates/noon-typst/benches/typst.rs",
    "crates/noon-typst/build.rs",
    "crates/noon-typst/Cargo.toml",
  ]) {
    assert.equal(selectsProvider([changed]), true, `provider workflow must select ${changed}`);
  }
});

test("provider qualification ignores unrelated docs and workflow-only changes", () => {
  for (const changed of [
    "README.md",
    "docs/architecture.md",
    ".github/workflows/provider-features.yml",
  ]) {
    assert.equal(selectsProvider([changed]), false, `provider workflow must ignore ${changed}`);
  }
});
