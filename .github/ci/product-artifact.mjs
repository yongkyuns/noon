// Same-run release artifacts for the product gate. Reuse the dev artifact contract.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { config, prepareArtifact, stamp, verify } from "./wasm-build.mjs";

export function productConfig(role) {
  assert.ok(["baseline", "candidate"].includes(role), "invalid product artifact role");
  return { ...config, profile: "release", features: role === "candidate" ? "default,renderer-smoke" : "default" };
}

async function main() {
  const [command, role, checkout] = process.argv.slice(2);
  const build = productConfig(role);
  assert.ok(checkout, "missing product checkout");
  const root = path.resolve(checkout);
  // Both checkouts use immutable event SHAs. The candidate is the tested merge,
  // never a substituted PR head; the baseline is the event's pinned base commit.
  const sha = role === "baseline" ? process.env.NOON_PRODUCT_BASE_SHA : process.env.GITHUB_SHA;
  assert.match(sha ?? "", /^[0-9a-f]{40}$/, "missing product source SHA");
  const env = { ...process.env, GITHUB_SHA: sha };
  const identityPath = path.join(root, "ci-artifacts/product-build.json");
  if (command === "prepare") {
    const compiler = spawnSync("rustc", ["-vV"], { cwd: root, encoding: "utf8" });
    assert.equal(compiler.status, 0, "rustc -vV failed");
    const identity = await prepareArtifact(root, env, compiler.stdout.trim(), build);
    await mkdir(path.dirname(identityPath), { recursive: true });
    await writeFile(identityPath, JSON.stringify(identity));
  } else if (command === "stamp") {
    await stamp(root, JSON.parse(await readFile(identityPath, "utf8")), env, build);
  } else if (command === "verify") {
    const manifest = await verify(root, env, build);
    console.log(`Verified ${role} release artifact for ${manifest.source}: ${Object.keys(manifest.files).length} files.`);
  } else throw new Error("expected prepare, stamp, or verify");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}
