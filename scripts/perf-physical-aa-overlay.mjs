// #1933: exactly one predeclared measurement-harness overlay for PHYSICAL GPU A/A.
// No changes to Noon production binaries, authored samples, FPS or cost scorers.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const PHYSICAL_HARNESS = Object.freeze({
  checkout: "439c4c4d33096665afa68bb070baaba52bd20f43",
  tree: "3cc0f65789861939992bf5e42b0095a4f71b74a1",
  originalPairBlob: "8322d5e626070b027d56d1bfb7580fc57eda4714",
  physicalPairBlob: "3af9df9684b5bf6b5b7b659f0606b0b3b9df594e",
  path: "scripts/playground-product-pair.mjs",
});

export function gitBlobSha(content) {
  assert.equal(typeof content, "string");
  const data = Buffer.from(content, "utf8");
  return createHash("sha1").update(`blob ${data.length}\0`).update(data).digest("hex");
}

export function physicalPairSource(original) {
  assert.equal(gitBlobSha(original), PHYSICAL_HARNESS.originalPairBlob,
    "only exact archived Apple diagnostic product pair harness is eligible");
  const from = `      assert.match(proof.unmaskedRenderer, /Apple.*Metal Renderer.*Apple Paravirtual/i,
        "actual Apple Paravirtual Metal renderer not observed");`;
  const to = `      assert.match(proof.unmaskedRenderer, /Apple.*Metal Renderer/i,
        "actual physical Apple Metal renderer not observed");
      assert.doesNotMatch(proof.unmaskedRenderer,
        /paravirtual|virtual device|swiftshader|llvmpipe|software/i,
        "virtualized or software GPU is not eligible for physical A/A");
      assert.ok(process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER,
        "physical readiness must pin the actual unmasked renderer before collection");
      assert.equal(proof.unmaskedRenderer, process.env.NOON_PHYSICAL_AA_EXPECTED_RENDERER,
        "A/A renderer no longer matches the independently observed readiness renderer");`;
  assert.equal(original.split(from).length, 2, "expected one physical GPU identity source guard");
  const oldArch = '      assert.equal(proof.arch, "arm64", "not standard hosted Apple Silicon arm64");';
  assert.equal(original.split(oldArch).length, 2, "expected one Apple Silicon guard");
  const transformed = original.replace(from, to).replace(oldArch,
    '      assert.equal(proof.arch, "arm64", "not physical Apple Silicon arm64");');
  assert.equal(gitBlobSha(transformed), PHYSICAL_HARNESS.physicalPairBlob,
    "unregistered harness overlay changed: no benchmark/renderer substitutions allowed");
  return transformed;
}

function git(root, ...args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export async function verifyPhysicalHarness(root, { apply = false } = {}) {
  assert.equal(git(root, "rev-parse", "HEAD"), PHYSICAL_HARNESS.checkout,
    "frozen measurement harness checkout changed");
  assert.equal(git(root, "rev-parse", "HEAD^{tree}"), PHYSICAL_HARNESS.tree,
    "frozen measurement harness Git tree changed");
  const original = execFileSync("git", ["show", `HEAD:${PHYSICAL_HARNESS.path}`],
    { cwd: root, encoding: "utf8" });
  const transformed = physicalPairSource(original);
  const target = path.join(root, PHYSICAL_HARNESS.path);
  const actual = await readFile(target, "utf8");
  if (apply) {
    assert.equal(actual, original, "measurement harness was already modified");
    await writeFile(target, transformed);
  } else {
    assert.equal(actual, transformed, "physical WebGL identity guard was altered");
  }
  const changed = git(root, "diff", "--name-only", "HEAD", "--").split("\n").filter(Boolean);
  assert.deepEqual(changed, [PHYSICAL_HARNESS.path],
    "modified tracked measurement harness paths exceed the registered GPU proof edit");
  assert.equal(gitBlobSha(await readFile(target, "utf8")), PHYSICAL_HARNESS.physicalPairBlob);
  return { qualification: false, mergeApproval: false, harness: PHYSICAL_HARNESS.checkout,
    originalPairBlob: PHYSICAL_HARNESS.originalPairBlob,
    physicalPairBlob: PHYSICAL_HARNESS.physicalPairBlob };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [mode, root] = process.argv.slice(2);
  assert.ok(["--apply", "--verify"].includes(mode) && root && process.argv.length === 4,
    "usage: node perf-physical-aa-overlay.mjs --apply|--verify FROZEN_HARNESS_DIR");
  const report = await verifyPhysicalHarness(path.resolve(root), { apply: mode === "--apply" });
  console.log(JSON.stringify(report));
}
