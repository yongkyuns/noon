// A PR's event base can predate the master included in its tested merge.
// Compare against that merge's first parent, never a moving branch or merge-base.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function productBaseline(root, candidate, head) {
  for (const sha of [candidate, head]) assert.match(sha ?? "", /^[0-9a-f]{40}$/, "missing immutable PR identity");
  const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  assert.equal(git("rev-parse", "HEAD"), candidate, "candidate checkout differs from tested merge");
  const parents = git("cat-file", "-p", candidate).split("\n").filter(line => line.startsWith("parent ")).map(line => line.slice(7));
  assert.equal(parents.length, 2, "product comparison requires the two-parent PR merge");
  assert.equal(parents[1], head, "PR head is not the tested merge's second parent");
  git("cat-file", "-e", `${parents[0]}^{commit}`); // Require fetched history; no fallback.
  return parents[0];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [root, candidate, head] = process.argv.slice(2);
  console.log(`base-sha=${productBaseline(root, candidate, head)}`);
}
