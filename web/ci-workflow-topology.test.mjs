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
    "crates/noon-compile/src/lib.rs",
    "crates/noon-runtime/src/frame.rs",
    "crates/noon-geometry/src/lib.rs",
    "crates/noon-text/src/lib.rs",
    "crates/noon-typst/src/lib.rs",
    "crates/noon-text/fonts/fixture.ttf",
    // New/moved provider packages must not need a historical crate-name allowlist.
    "crates/new-provider/src/lib.rs",
    "crates/new-provider/build.rs",
    "crates/new-provider/Cargo.toml",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
    ".cargo/config",
    ".cargo/config.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    "fixtures/clippy.toml",
    "fixtures/.rustfmt.toml",
    "fixtures/provider-consumer/Cargo.toml",
    "fixtures/provider-consumer/Cargo.lock",
    "fixtures/provider-consumer/.cargo/config.toml",
    "fixtures/provider-consumer/src/main.rs",
    "fixtures/provider-consumer/tests/providers.rs",
    "fixtures/provider-consumer/tests/public_facade.rs",
    "fixtures/provider-consumer/fonts/fixture.ttf",
    "scripts/provider-features.py",
    "scripts/provider_features_test.py",
    "web/ci-workflow-topology.test.mjs",
    ".github/workflows/provider-features.yml",
  ]) {
    assert.equal(selectsProvider([changed]), true, `provider qualification skipped ${changed}`);
  }
});

test("provider selection excludes unrelated edits and handles mixed changes", () => {
  const irrelevant = [
    "README.md", "docs/architecture.md", "assets/hello_world.gif",
    "web/python/_manim_compat.py", "web/index.html", "scripts/browser-smoke.mjs",
    ".github/workflows/pages.yml", "fixtures/unrelated/input.json",
    "crates-other/noon/src/lib.rs", "fixtures/provider-consumer-other/src/main.rs",
    "scripts/provider_features_test.py.bak", "Cargo.toml.bak",
  ];
  for (const changed of irrelevant) assert.equal(selectsProvider([changed]), false, changed);
  assert.equal(selectsProvider([]), false);
  assert.equal(selectsProvider(irrelevant), false);
  assert.equal(selectsProvider([...irrelevant, "crates/noon/examples/provider_probe.rs"]), true);
});

test("provider selector preserves glob boundaries and fails closed on new syntax", () => {
  for (const changed of ["crates/Cargo.toml", "crates/noon/Cargo.toml", "crates/nested/noon/Cargo.toml"]) {
    assert.equal(providerPathMatches("crates/**/Cargo.toml", changed), true);
  }
  assert.equal(providerPathMatches("crates/**", "crates/.hidden/input.rs"), true);
  assert.equal(providerPathMatches("crates/**", "other/crates/noon/src/lib.rs"), false);
  assert.equal(providerPathMatches("crates/**/Cargo.toml", "crates/noon/Cargo.toml.bak"), false);
  assert.equal(providerPathMatches("Cargo.toml", "CargoXtoml"), false);
  assert.throws(() => providerPathMatches("crates/*/src/**", "crates/noon/src/lib.rs"), /unsupported/);
  assert.throws(() => providerPullRequestPaths(providerWorkflow.replace("    paths:", "    paths-ignore:")), /paths-only/);
  assert.throws(() => providerPullRequestPaths(providerWorkflow.replace("    paths:", "    branches: [master]\n    paths:")), /paths-only/);
  assert.throws(() => providerPullRequestPaths(providerWorkflow.replace("      - 'Cargo.toml'", "      - '!Cargo.toml'")), /unsupported/);
});

test("provider workflow retains cached five-by-two qualification and example regression", () => {
  assert.match(providerWorkflow, /config: \[minimal, native-text, native-bundled, typst, product\]/);
  assert.match(providerWorkflow, /target: \[x86_64-unknown-linux-gnu, wasm32-unknown-unknown\]/);
  assert.match(providerWorkflow, /RUSTC_WRAPPER: \$\{\{ !inputs\.measure && 'sccache' \|\| '' \}\}/);
  assert.match(providerWorkflow, /SCCACHE_GHA_RW_MODE: READ_ONLY/);
  assert.match(providerWorkflow, /if: github\.event_name == 'workflow_dispatch' && inputs\.measure/);
  assert.match(providerWorkflow, /cargo test -p noon --no-default-features --doc/);
  assert.match(providerWorkflow, /cargo run -p noon --no-default-features --example shared_authoring/);
  const probe = providerWorkflow.split("      - name: Prove provider-dependent examples require feature guards\n")[1]?.split("\n      - ")[0];
  assert.ok(probe, "provider example regression must run in the real workflow");
  assert.match(probe, /if: matrix\.config == 'minimal' && matrix\.target == 'x86_64-unknown-linux-gnu'/);
  assert.match(probe, /NOON_PROVIDER_COMPILE_TESTS: '1'/);
  assert.match(probe, /python3 scripts\/provider_features_test\.py ProviderExampleTests/);
});


test("architecture diagrams stay a read-only check with independently preserved artifacts", async () => {
  const workflow = await readFile(new URL("architecture-diagrams.yml", workflowDir), "utf8");
  assert.match(workflow, /permissions:\n  contents: read/);
  assert.doesNotMatch(workflow, /^\s*(contents|actions|pull-requests):\s*write\b/m);
  assert.match(workflow, /persist-credentials: false/);
  assert.doesNotMatch(workflow, /\bgit\s+(push|commit)\b/);
  assert.match(workflow, /python3 scripts\/test_architecture_diagrams\.py/);
  assert.match(workflow, /python3 scripts\/architecture_diagrams\.py --check/);
  assert.match(workflow, /sha256sum -c -/);
  const preservation = workflow.split("      - name: Preserve diagram sources and checked-in SVGs\n")[1];
  assert.ok(preservation, "diagram artifacts must remain available when checks fail");
  assert.match(preservation, /^        if: always\(\)$/m);
  assert.match(preservation, /uses: actions\/upload-artifact@/);
  assert.match(preservation, /docs\/diagrams\//);
});
