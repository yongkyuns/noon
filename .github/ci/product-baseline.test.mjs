import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { productBaseline } from "./product-baseline.mjs";

test("product baseline is the tested merge parent, not the stale PR branch point", async t => {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-product-base-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "-q", "-b", "master");
  git("config", "user.name", "Test"); git("config", "user.email", "test@example.invalid");
  const commit = async (name) => {
    await writeFile(path.join(root, name), name); git("add", name); git("commit", "-qm", name);
    return git("rev-parse", "HEAD");
  };
  const oldBase = await commit("base");
  git("checkout", "-qb", "topic"); const head = await commit("topic");
  git("checkout", "-q", "master"); const newBase = await commit("new-master");
  git("merge", "--no-ff", "-qm", "tested merge", "topic"); const merge = git("rev-parse", "HEAD");
  assert.notEqual(oldBase, newBase);
  assert.equal(productBaseline(root, merge, head), newBase);
  assert.throws(() => productBaseline(root, merge, oldBase), /second parent/);
  assert.throws(() => productBaseline(root, head, head), /checkout differs/);
  assert.throws(() => productBaseline(root, "master", head), /immutable PR identity/);
  git("checkout", "-q", head);
  assert.throws(() => productBaseline(root, head, head), /two-parent/);
});


test("product comparison uses the tested merge's pinned master parent in both jobs", async () => {
  const workflow = await readFile(new URL("../workflows/playground-product-gate.yml", import.meta.url), "utf8");
  assert.match(workflow, /fetch-depth: 2/);
  assert.match(workflow, /product-baseline\.mjs candidate/);
  assert.match(workflow, /baseline-sha: \$\{\{ steps\.comparison-base\.outputs\.base-sha \}\}/);
  assert.match(workflow, /NOON_PRODUCT_BASE_SHA: \$\{\{ needs\.build\.outputs\.baseline-sha \}\}/);
  assert.doesNotMatch(workflow, /ref: \$\{\{ github\.event\.pull_request\.base\.sha/);
  assert.match(workflow, /node scripts\/python-host-perf\.mjs/);
});
