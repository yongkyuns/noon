import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, renameSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { needsCudaDemoQualification, needsFullCiQualification, needsPagesDeployment } from "../../scripts/ci-qualification-scope.mjs";

const workflow = await readFile(
  new URL("../workflows/manim-raster-differential.yml", import.meta.url),
  "utf8",
);
const triggers = workflow.split("on:\n")[1]?.split("\npermissions:")[0];
const preflight = workflow.split("\n  preflight:\n")[1]?.split("\n  raster:\n")[0];
const oracle = workflow.split("\n  raster:\n")[1];

test("native Manim comparisons install the repository toolchain in job-private storage", async () => {
  const source = await readFile(new URL("../workflows/manim-differential.yml", import.meta.url), "utf8");
  const job = source.split("\n  semantics:\n")[1];
  assert.ok(job, "missing native semantic comparisons");
  assert.match(job.split("\n    steps:\n")[0], /RUSTUP_HOME: \$\{\{ runner\.temp \}\}\/noon-rustup/,
    "all Rust subprocesses must use storage isolated from preinstalled toolchains");
  const setup = job.split("      - name: Install pinned Rust toolchain\n")[1]?.split("      - name:")[0];
  assert.ok(setup, "install before the comparison scripts invoke Cargo");
  assert.match(setup, /^          rustup install$/m, "select rust-toolchain.toml, including its components and targets");
  assert.match(setup, /^          rustup show active-toolchain$/m);
  assert.match(setup, /^          rustc --version --verbose$/m);
  assert.ok(job.indexOf("Install pinned Rust toolchain") < job.indexOf("Compare native geometry"));
  for (const probe of ["dot-ellipse", "sector", "elbow", "frame-sampling"]) {
    assert.ok(job.includes(`python scripts/manim-${probe}-differential.py`), `missing original ${probe} comparison`);
  }
});

test("native functional gates supersede PR revisions while master runs stay independent", async () => {
  for (const name of ["native-host-smoke.yml", "native-export-output.yml"]) {
    const source = await readFile(new URL("../workflows/" + name, import.meta.url), "utf8");
    const concurrency = source.split("\nconcurrency:\n")[1]?.split("\njobs:\n")[0];
    assert.ok(concurrency, name + ": missing obsolete-run cancellation policy");
    assert.match(concurrency,
      /group: \$\{\{ github\.workflow \}\}-\$\{\{ github\.event\.pull_request\.number \|\| github\.run_id \}\}/,
      "isolate workflows and PRs; each non-PR run must have its own group");
    assert.match(concurrency,
      /cancel-in-progress: \$\{\{ github\.event_name == 'pull_request' \}\}/,
      "only superseded PR revisions may cancel active functional qualification");
  }
});

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

test("Pages skips known non-product changes using the complete push diff", async () => {
  const nonProduct = ["crates/noon/tests/family/main.rs", "crates/noon-web/tests/lifecycle.rs",
    ".github/ci/wasm-build.test.mjs", ".github/ci/README.md", ".github/workflows/pr-fast.yml",
    ".github/workflows/playground-product-gate.yml"];
  assert.equal(needsPagesDeployment(revisions, diffFixture(nonProduct)), false);
  const pages = await readFile(new URL("../workflows/pages.yml", import.meta.url), "utf8");
  for (const path of ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml",
    "crates/noon/src/family_layout.rs", "crates/new-engine/src/lib.rs", "web/index.html",
    "web/python/examples/new_scene.py", "scripts/build-web-demo.sh", "new-product-input.bin",
    ".github/ci/wasm-build.mjs", "scripts/ci-qualification-scope.mjs", ".github/workflows/pages.yml",
    ...[...pages.matchAll(/uses: \.\/(\S+)/g)].map(match => match[1])]) {
    assert.equal(needsPagesDeployment(revisions, diffFixture([path])), true, path);
    assert.equal(needsPagesDeployment(revisions, diffFixture([...nonProduct, path])), true, "mixed: " + path);
  }
  const large = Array.from({ length: 3001 }, (_, i) => `crates/noon/tests/entry-${i}.rs`);
  assert.equal(needsPagesDeployment(revisions, diffFixture(large)), false);
  assert.equal(needsPagesDeployment(revisions, diffFixture([...large, "web/main.js"])), true);
  const push = pages.split("  push:\n")[1]?.split("  workflow_dispatch:")[0];
  assert.doesNotMatch(push, /paths(?:-ignore)?:/, "do not use the bounded event path list");
  assert.match(pages, /run: node scripts\/ci-qualification-scope\.mjs pages/);
});

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
    assert.equal(needsPagesDeployment({ ...revisions, event }, () => assert.fail("must not read a diff")), true);
  }
});

test("missing revisions, diff failures and incomplete evidence retain full CI", () => {
  for (const qualify of [needsFullCiQualification, needsPagesDeployment]) {
    for (const field of ["before", "after"]) {
      for (const value of [undefined, "0".repeat(40), "HEAD", "bad;revision"]) {
        assert.equal(qualify({ ...revisions, [field]: value }, () => assert.fail("invalid revision reached Git")), true);
      }
    }
    for (const output of [Buffer.alloc(0), Buffer.from("\0"), Buffer.from("README.md\0\0"), Buffer.from("README.md"), "README.md\0"]) {
      assert.equal(qualify(revisions, (_command, args) => args[0] === "diff" ? output : Buffer.alloc(0)), true);
    }
    for (const failingCommand of ["fetch", "diff"]) {
      assert.equal(qualify(revisions, (_command, args) => {
        if (args[0] === failingCommand) throw new Error("missing evidence");
        return Buffer.alloc(0);
      }), true);
    }
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
  assert.equal(readFileSync(output, "utf8"), "full=false\ncuda_demo=false\n");
  // Keep generated Action output outside the committed source fixture.
  rmSync(output);
  mkdirSync(join(root, "crates"));
  renameSync(join(root, "docs/example.md"), join(root, "crates/new.rs"));
  execFileSync(process.execPath, [script], {
    cwd: root, stdio: "pipe", env: { ...env, BEFORE_REVISION: after, GITHUB_SHA: commit() },
  });
  assert.equal(readFileSync(output, "utf8"), "full=true\ncuda_demo=false\n");
  rmSync(output);
  const runtime = git("rev-parse", "HEAD").toString().trim();
  mkdirSync(join(root, "crates/noon/tests"), { recursive: true });
  writeFileSync(join(root, "crates/noon/tests/fixture.rs"), "// integration test\n");
  const testsOnly = commit();
  execFileSync(process.execPath, [script, "pages"], {
    cwd: root, stdio: "pipe", env: { ...env, BEFORE_REVISION: runtime, GITHUB_SHA: testsOnly },
  });
  assert.equal(readFileSync(output, "utf8"), "deploy=false\n");
  rmSync(output);
  renameSync(join(root, "crates/noon/tests/fixture.rs"), join(root, "crates/new-production.rs"));
  execFileSync(process.execPath, [script, "pages"], {
    cwd: root, stdio: "pipe", env: { ...env, BEFORE_REVISION: testsOnly, GITHUB_SHA: commit() },
  });
  assert.equal(readFileSync(output, "utf8"), "deploy=true\n");
});


test("CUDA fidelity is selected for source, controls, renames and cadence changes", () => {
  for (const path of ["web/python/examples/cuda_transpose_banks.py", "scripts/cuda-transpose-qualification.mjs",
    "scripts/cuda-transpose-pixels.test.mjs", "tests/cuda_visual_validation_test.py",
    "tools/cuda-transpose/visual_probe.py", ".github/workflows/ci.yml",
    ".github/workflows/manim-raster-differential.yml", "scripts/ci-qualification-scope.mjs"]) {
    assert.equal(needsCudaDemoQualification(revisions, diffFixture(["docs/old.md", path])), true, path);
  }
  assert.equal(needsCudaDemoQualification(revisions, diffFixture(["crates/noon/src/lib.rs"])), false,
    "generic engine edits retain ordinary full CI and nightly CUDA fidelity");
  for (const event of ["workflow_dispatch", "schedule", "unknown"]) {
    assert.equal(needsCudaDemoQualification({ ...revisions, event }, () => assert.fail("no diff needed")), true);
  }
});

test("CUDA PR scope uses the actual merge parent even with stale event base metadata", () => {
  const base = "c".repeat(40), head = "d".repeat(40), calls = [];
  const runGit = (command, args) => {
    calls.push([command, args]);
    if (args[0] === "cat-file") return Buffer.from(`tree ${"e".repeat(40)}\nparent ${base}\nparent ${head}\n\nparent ${revisions.before}\n`);
    return diffFixture(["web/main.js"])(command, args);
  };
  assert.equal(needsCudaDemoQualification({ ...revisions, event: "pull_request" }, runGit), false);
  assert.deepEqual(calls[1][1], ["fetch", "--no-tags", "--depth=1", "origin", base]);
  assert.deepEqual(calls[2][1], ["diff", "--name-only", "--no-renames", "-z", base, revisions.after]);
});

test("missing or malformed CUDA scope evidence cannot skip the full fidelity suite", () => {
  for (const output of [Buffer.alloc(0), Buffer.from("README.md"), Buffer.from("README.md\0\0"), "README.md\0"]) {
    assert.equal(needsCudaDemoQualification(revisions, (_command, args) => args[0] === "diff" ? output : Buffer.alloc(0)), true);
  }
  for (const output of [Buffer.from("tree x\n\nmessage"), Buffer.from(`parent ${revisions.before}\n\nmessage`), "not a buffer"]) {
    assert.equal(needsCudaDemoQualification({ ...revisions, event: "pull_request" }, () => output), true);
  }
  assert.equal(needsCudaDemoQualification(revisions, () => { throw new Error("missing history"); }), true);
});

test("CUDA selection reaches the browser consumer through the existing build output", async () => {
  const ci = await readFile(new URL("../workflows/ci.yml", import.meta.url), "utf8");
  const build = ci.split("  web-build:\n")[1].split("  browser-authoring:")[0];
  const consumer = ci.split("  browser-authoring:\n")[1].split("  browser-reactive:")[0];
  assert.match(build, /cuda_demo: \$\{\{ needs\.qualification\.outputs\.cuda_demo \}\}/);
  assert.match(consumer, /needs: web-build/);
  assert.match(consumer, /if: \$\{\{ needs\.web-build\.outputs\.cuda_demo != 'false' \}\}/,
    "missing classifier output must execute the suite, not skip it");
});
