import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { needsFullCiQualification } from "../../scripts/ci-qualification-scope.mjs";

const workflow = await readFile(
  new URL("../workflows/manim-raster-differential.yml", import.meta.url),
  "utf8",
);
const triggers = workflow.split("on:\n")[1]?.split("\npermissions:")[0];
const preflight = workflow.split("\n  preflight:\n")[1]?.split("\n  raster:\n")[0];
const oracle = workflow.split("\n  raster:\n")[1];

test("full Manim raster qualification runs nightly and by explicit dispatch, not per merge", () => {
  assert.ok(triggers, "missing event configuration");
  assert.match(triggers, /  schedule:\n    - cron: "17 6 \* \* \*"/);
  assert.match(triggers, /  workflow_dispatch:/);
  assert.match(triggers, /  pull_request:/);
  assert.doesNotMatch(triggers, /^  push:/m, "full oracle must not be retriggered after every merge");
  assert.match(oracle, /if: \$\{\{ github\.event_name == 'schedule' \|\| github\.event_name == 'workflow_dispatch' \}\}/);
  assert.match(workflow, /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' \}\}/);
});

test("relevant PRs still validate Manim policy, manifests and negative controls", () => {
  assert.ok(preflight, "missing fast PR policy job");
  assert.match(preflight, /if: \$\{\{ github\.event_name == 'pull_request' \}\}/);
  assert.match(preflight, /node scripts\/manim-raster-policy\.test\.mjs/);
  assert.match(preflight, /node --test scripts\/manim-raster-samples\.test\.mjs/);
  assert.match(preflight, /python3 -m json\.tool parity\/manim-v0\.21\/manifest\.json/);
  assert.match(preflight, /python3 -m py_compile scripts\/test_manim_raster_manifest\.py/);
  assert.match(preflight, /scripts\/special-camera-qualification\.test\.mjs/);
  assert.doesNotMatch(preflight, /build-web-demo|apt-get|pip install|playwright install/);
});

test("nightly and manually dispatched Manim gate retain full strict checks", () => {
  assert.ok(oracle, "missing full Manim raster job");
  for (const step of [
    "Install pinned ManimCE reference",
    "Build browser package",
    "Qualify pinned FollowingGraphCamera",
    "Render and compare retained Typst fixtures",
    "Enforce retained Typst raster ratchets",
    "Render and compare canonical Manim scenes",
    "Capture semantic state at raster frame times",
    "Enforce per-fixture raster ratchet",
    "Verify shared sparse and dense playback at Manim frame times",
    "Report deferred qualification failures",
  ]) {
    assert.ok(oracle.includes("      - name: " + step), "removed strict gate stage: " + step);
  }
  assert.match(oracle, /NOON_MANIM_RASTER_BACKENDS: webgpu,webgl/);
  assert.match(oracle, /if: \$\{\{ always\(\) && \(steps\.following_graph_camera\.outcome == 'failure'/);
});

test("unrelated CI policies do not select heavyweight product or browser qualification", async () => {
  const unrelated = [
    "README.md", "manim-raster-workflow-cadence.test.mjs",
    "run-report.mjs", "run-report.test.mjs", "physical-gpu-mode.test.mjs",
  ];
  for (const name of ["ci.yml", "playground-product-gate.yml"]) {
    const source = await readFile(new URL("../workflows/" + name, import.meta.url), "utf8");
    const events = source.split("on:\n")[1]?.split("\npermissions:")[0];
    const rules = [...events.matchAll(/^      - "(!?\.github\/ci\/[^\"]+)"$/gm)]
      .map(match => match[1]);
    assert.ok(rules.includes(".github/ci/**"), "new CI helpers must still select qualification");
    const selected = file => rules.reduce((included, rule) => {
      const exclude = rule.startsWith("!");
      const pattern = exclude ? rule.slice(1) : rule;
      assert.ok(pattern === ".github/ci/**" || !pattern.includes("*"), "unsupported CI policy glob");
      return pattern === ".github/ci/**" || pattern === file ? !exclude : included;
    }, false);
    for (const file of unrelated) assert.equal(selected(".github/ci/" + file), false, name + ": " + file);
    for (const file of ["wasm-build.mjs", "product-artifact.mjs", "product-cohort-topology.test.mjs", "new-helper.mjs"]) {
      assert.equal(selected(".github/ci/" + file), true, name + ": " + file);
    }
    assert.equal(["README.md", "wasm-build.mjs"].some(file => selected(".github/ci/" + file)), true,
      "a mixed policy/production change must still select qualification");
    if (name === "playground-product-gate.yml") {
      assert.equal(selected(".github/ci/gallery-optimization-topology.test.mjs"), false);
    }
  }
  const fast = await readFile(new URL("../workflows/pr-fast.yml", import.meta.url), "utf8");
  assert.match(fast, /on:\n  pull_request:\n\npermissions:/);
  assert.ok(fast.includes("node --test .github/ci/*.test.mjs"), "excluded policies must still run on every PR");
});

test("web unit-test edits stay in preflight without selecting full gallery or performance runs", async () => {
  for (const [name, count] of [
    ["playground-product-gate.yml", 1], ["playground-cross-browser.yml", 2],
    ["playground-authoring-recovery.yml", 2], ["playground-resize-recovery.yml", 2],
    ["playground-lifecycle-recovery.yml", 1], ["playground-playback-controls.yml", 1],
  ]) {
    const source = await readFile(new URL("../workflows/" + name, import.meta.url), "utf8");
    const events = source.split("on:\n")[1]?.split("\npermissions:")[0];
    assert.equal(events.split('      - "web/**"\n      - "!web/**/*.test.mjs"').length - 1, count,
      name + ": exclude test sources after the broad web selector on each automatic event");
  }
  const playback = await readFile(new URL("../workflows/playground-playback-controls.yml", import.meta.url), "utf8");
  const pr = playback.split("  pull_request:\n")[1]?.split("  workflow_dispatch:")[0];
  assert.ok(pr?.trimEnd().endsWith('      - "!web/**/*.test.mjs"'),
    "playback test names matching runtime prefixes must stay in preflight");
});

const revisions = { event: "push", before: "a".repeat(40), after: "b".repeat(40) };
const diffFixture = paths => (_command, args) =>
  Buffer.from(args[0] === "diff" ? paths.join("\0") + "\0" : "");

test("master docs, named policy and web unit-test changes use preflight", () => {
  const paths = ["README.md", "AGENTS.md", "AGENTS.override.md", "docs/architecture.md",
    "docs/diagrams/execution.svg", ".github/ci/README.md",
    ".github/ci/manim-raster-workflow-cadence.test.mjs", ".github/ci/run-report.mjs",
    ".github/ci/run-report.test.mjs", ".github/ci/physical-gpu-mode.test.mjs",
    "web/playground-playback-controls.test.mjs", "web/nested/example.test.mjs"];
  assert.equal(needsFullCiQualification(revisions, diffFixture(paths)), false);
});

test("unknown, runtime, qualification and mixed changes retain complete master CI", () => {
  for (const path of ["new-input.dat", "Cargo.toml", "rust-toolchain.toml",
    "crates/noon/src/lib.rs", "web/main.js", "web/python/noon.py",
    "web/ci-workflow-topology.test.mjs", "scripts/ci-qualification-scope.mjs",
    ".github/workflows/ci.yml", ".github/ci/wasm-build.mjs", ".github/ci/new-helper.mjs"]) {
    assert.equal(needsFullCiQualification(revisions, diffFixture(["docs/architecture.md", path])), true, path);
  }
  // Do not depend on the host's bounded changed-file list or newline filenames.
  const large = Array.from({ length: 3001 }, (_, i) => `docs/entry-${i}.md`);
  assert.equal(needsFullCiQualification(revisions, diffFixture([...large, "web/main.js"])), true);
  assert.equal(needsFullCiQualification(revisions, diffFixture(["README.md\nweb/main.js"])), true);
});

test("manual and PR qualification cannot be suppressed by path classification", () => {
  for (const event of ["pull_request", "workflow_dispatch", "unknown"]) {
    assert.equal(needsFullCiQualification({ ...revisions, event }, () => assert.fail("must not read a diff")), true);
  }
});

test("missing revisions, diff failures and incomplete evidence retain full CI", () => {
  for (const field of ["before", "after"]) {
    for (const value of [undefined, "0".repeat(40), "HEAD", "bad;revision"]) {
      assert.equal(needsFullCiQualification({ ...revisions, [field]: value }, () => assert.fail("invalid revision reached Git")), true);
    }
  }
  for (const output of [Buffer.alloc(0), Buffer.from("\0"), Buffer.from("README.md\0\0"), Buffer.from("README.md"), "README.md\0"]) {
    assert.equal(needsFullCiQualification(revisions, (_command, args) => args[0] === "diff" ? output : Buffer.alloc(0)), true);
  }
  for (const failingCommand of ["fetch", "diff"]) {
    assert.equal(needsFullCiQualification(revisions, (_command, args) => {
      if (args[0] === failingCommand) throw new Error("missing evidence");
      return Buffer.alloc(0);
    }), true);
  }
});

test("master classification compares the exact full push without rename filtering", () => {
  const calls = [];
  assert.equal(needsFullCiQualification(revisions, (command, args) => {
    calls.push([command, args]);
    return diffFixture(["docs/moved.md", "crates/noon/src/moved.rs"])(command, args);
  }), true);
  assert.deepEqual(calls, [
    ["git", ["fetch", "--no-tags", "--depth=1", "origin", revisions.before]],
    ["git", ["diff", "--name-only", "--no-renames", "-z", revisions.before, revisions.after]],
  ]);
});

test("master CI suppresses heavy work only on an explicit successful cheap classification", async () => {
  const ci = await readFile(new URL("../workflows/ci.yml", import.meta.url), "utf8");
  const push = ci.split("  push:\n")[1]?.split("  pull_request:")[0];
  assert.doesNotMatch(push, /paths(?:-ignore)?:/, "master qualification uses the complete Git diff");
  assert.match(ci, /run: node scripts\/ci-qualification-scope\.mjs/);
  assert.match(ci, /BEFORE_REVISION: \$\{\{ github\.event\.before \}\}/);
  for (const id of ["rust", "glow-raster", "web-build"]) {
    const job = ci.split(`\n  ${id}:\n`)[1]?.split(/^  [a-z][a-z-]*:/m)[0];
    assert.match(job, /needs: qualification/);
    assert.match(job, /if: \$\{\{ !cancelled\(\) && \(needs\.qualification\.result != 'success' \|\| needs\.qualification\.outputs\.full != 'false'\) \}\}/,
      "failed/skipped classification must not silently skip heavy checks");
  }
});

test("real Git and CLI classify docs and a rename into runtime source", t => {
  const root = mkdtempSync(join(tmpdir(), "noon-ci-scope-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe" });
  const commit = () => {
    git("add", ".");
    git("-c", "user.name=CI fixture", "-c", "user.email=ci-fixture@example.invalid",
      "commit", "--quiet", "-m", "fixture");
    return git("rev-parse", "HEAD").toString().trim();
  };
  git("init", "--quiet");
  git("remote", "add", "origin", root);
  mkdirSync(join(root, "docs"));
  writeFileSync(join(root, "docs/example.md"), "first\n");
  const before = commit();
  writeFileSync(join(root, "docs/example.md"), "second\n");
  const after = commit();
  const script = fileURLToPath(new URL("../../scripts/ci-qualification-scope.mjs", import.meta.url));
  const output = join(root, "output.txt");
  const env = { ...process.env, GITHUB_EVENT_NAME: "push", BEFORE_REVISION: before,
    GITHUB_SHA: after, GITHUB_OUTPUT: output };
  execFileSync(process.execPath, [script], { cwd: root, env, stdio: "pipe" });
  assert.equal(readFileSync(output, "utf8"), "full=false\n");
  // Keep generated Action output outside the committed source fixture.
  rmSync(output);
  mkdirSync(join(root, "crates"));
  renameSync(join(root, "docs/example.md"), join(root, "crates/new.rs"));
  execFileSync(process.execPath, [script], {
    cwd: root, stdio: "pipe", env: { ...env, BEFORE_REVISION: after, GITHUB_SHA: commit() },
  });
  assert.equal(readFileSync(output, "utf8"), "full=true\n");
});
