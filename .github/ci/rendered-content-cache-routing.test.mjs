// #2094: test workflow selection, not GPU correctness or production cache admission.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { needsFullCiQualification } from "../../scripts/ci-qualification-scope.mjs";

const workflow = readFileSync(new URL("../workflows/ci.yml", import.meta.url), "utf8");
const probes = [
  "crates/noon-render-wgpu/tests/rendered_content_cache_msaa.rs",
  "crates/noon-render-wgpu/tests/rendered_content_cache_msaa.wgsl",
];

// This workflow uses a simple quoted paths-only PR selector. Fail closed on new
// YAML/event syntax; this is not a second general workflow parser.
function prPaths(source) {
  const lines = source.split("\n").filter(line => line.trim() && !line.trimStart().startsWith("#"));
  const on = lines.indexOf("on:");
  assert.ok(on >= 0);
  const end = lines.findIndex((line, i) => i > on && !line.startsWith(" "));
  const events = lines.slice(on + 1, end < 0 ? undefined : end);
  const pr = events.indexOf("  pull_request:");
  assert.ok(pr >= 0);
  const next = events.findIndex((line, i) => i > pr && /^  \S/.test(line));
  const filter = events.slice(pr + 1, next < 0 ? undefined : next);
  assert.equal(filter.shift(), "    paths:");
  assert.ok(filter.length > 0);
  return filter.map(line => {
    const match = /^      - (["'])(!?[A-Za-z0-9_./*-]+)\1(?:\s+#.*)?$/.exec(line);
    assert.ok(match, `unsupported CI path selector: ${line}`);
    return match[2];
  });
}

function matches(pattern, path) {
  const parts = pattern.split("/");
  const expression = parts.map((part, i) => {
    const last = i === parts.length - 1;
    if (part === "**") return last ? ".*" : "(?:[^/]+/)*";
    assert.ok(!part.includes("**"), `unsupported embedded globstar: ${pattern}`);
    assert.match(part, /^[A-Za-z0-9_.*-]+$/);
    return part.replaceAll(".", "\\.").replaceAll("*", "[^/]*") + (last ? "" : "/");
  }).join("");
  return new RegExp(`^${expression}$`).test(path);
}

function selected(patterns, path) {
  let result = false;
  for (const pattern of patterns) {
    const exclude = pattern.startsWith("!");
    if (matches(exclude ? pattern.slice(1) : pattern, path)) result = !exclude;
  }
  return result;
}

const paths = prPaths(workflow);

test("each cache probe source alone selects native raster CI", () => {
  for (const path of probes) {
    assert.ok(paths.includes(path), `missing explicit cache-probe ownership: ${path}`);
    assert.ok(selected(paths, path), `probe-only change must run CI: ${path}`);
  }
  for (const path of [
    "tests/visual-effects/qualify_gpu_operator.py",
    "tests/visual-effects/test_gpu_qualification.py",
    ".github/ci/rendered-content-cache-routing.test.mjs",
  ]) assert.ok(selected(paths, path), `qualification input must run CI: ${path}`);
});

test("removing either probe path reproduces the missing-trigger regression", () => {
  for (const path of probes) {
    const broken = workflow.replace(`      - "${path}"\n`, "");
    assert.notEqual(broken, workflow);
    assert.equal(selected(prPaths(broken), path), false, path);
  }
});

test("path checks respect exclusions, re-inclusions, segment boundaries and comments", () => {
  assert.equal(selected(["crates/**", "!crates/**"], probes[0]), false);
  assert.equal(selected(["crates/**", "!crates/**", probes[0]], probes[0]), true);
  assert.equal(matches("crates/*/tests/*.rs", probes[0]), true);
  assert.equal(matches("crates/*/tests/*.rs", "crates/a/b/tests/a.rs"), false);
  assert.equal(matches("crates/**/tests/*.rs", "crates/tests/a.rs"), true);
  assert.equal(selected(paths, ".github/ci/README.md"), false);
  assert.equal(selected(paths, "docs/cache.md"), false);
  const commented = workflow.replace(`      - "${probes[0]}"`, `      # - "${probes[0]}"`);
  assert.equal(selected(prPaths(commented), probes[0]), false);
  assert.throws(() => prPaths(workflow.replace("    paths:", "    paths-ignore:")));
});

test("probe-only PR and push changes cannot be classified as preflight-only", () => {
  assert.equal(needsFullCiQualification({ event: "pull_request" }), true);
  const revisions = { event: "push", before: "1".repeat(40), after: "2".repeat(40) };
  for (const path of probes) {
    const commands = [];
    const runGit = (command, args) => {
      assert.equal(command, "git");
      commands.push(args);
      return args[0] === "diff" ? Buffer.from(`${path}\0`) : Buffer.alloc(0);
    };
    assert.equal(needsFullCiQualification(revisions, runGit), true, path);
    assert.equal(commands.length, 2);
    assert.ok(commands[1].includes("--no-renames"));
  }
  assert.equal(needsFullCiQualification(revisions, (_command, args) =>
    args[0] === "diff" ? Buffer.from("docs/cache.md\0") : Buffer.alloc(0)), false);
});

test("selected CI retains the real raster runner and its always-uploaded evidence", () => {
  const raster = workflow.split("\n  glow-raster:\n")[1]?.split("\n  web-build:\n")[0];
  assert.ok(raster);
  assert.match(raster, /^    needs: qualification$/m);
  assert.match(raster, /^      NOON_GLOW_BACKEND: vulkan$/m);
  assert.match(raster, /^        run: python3 tests\/visual-effects\/qualify_gpu_operator\.py --report-dir glow-raster-artifacts$/m);
  assert.doesNotMatch(raster, /continue-on-error:/);
  const evidence = raster.split("      - name: Preserve native glow qualification evidence\n")[1];
  assert.ok(evidence);
  assert.match(evidence, /^        if: always\(\)$/m);
  assert.match(evidence, /^          path: glow-raster-artifacts$/m);
});
