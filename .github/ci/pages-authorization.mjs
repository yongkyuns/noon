// Deployment authorization is separate from build/qualification success.
// This helper only validates an explicit request; it never dispatches or deploys.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repository = "yongkyuns/noon";
const branch = "refs/heads/master";
const commitPattern = /^[0-9a-f]{40}$/;

export function authorizePagesDeployment({ event, env, checkoutSha }) {
  assert.equal(env.GITHUB_EVENT_NAME, "workflow_dispatch", "Pages requires an explicit manual dispatch");
  assert.equal(env.GITHUB_REPOSITORY, repository, "Pages request is for another repository");
  assert.equal(env.GITHUB_REF, branch, "Pages must be dispatched on master");
  assert.equal(event?.repository?.full_name, repository, "Pages event repository differs");
  assert.ok(event?.ref === "master" || event?.ref === branch, "Pages event must select the master branch");
  assert.ok(event?.inputs?.confirm_deploy === true || event?.inputs?.confirm_deploy === "true",
    "Pages deployment was not explicitly confirmed");
  const expected = event?.inputs?.expected_sha;
  assert.ok(typeof expected === "string" && commitPattern.test(expected),
    "expected_sha must be an exact, lowercase, full commit SHA");
  assert.equal(expected, env.GITHUB_SHA, "dispatch source differs from the approved commit");
  assert.equal(checkoutSha, expected, "checkout differs from the approved commit");
  return { schema: 1, repository, ref: branch, sourceRevision: expected,
    authorization: "explicit-demo-deployment", releaseApproval: false };
}

async function main() {
  assert.equal(process.argv.length, 2, "usage: node .github/ci/pages-authorization.mjs");
  const eventPath = process.env.GITHUB_EVENT_PATH;
  assert.ok(eventPath, "GITHUB_EVENT_PATH is required");
  const event = JSON.parse(await readFile(eventPath, "utf8"));
  // No input is interpolated into a shell command. Read the actual checkout,
  // rather than trusting only the workflow/event's claimed source revision.
  const checkoutSha = execFileSync("git", ["rev-parse", "--verify", "HEAD"],
    { encoding: "utf8" }).trim();
  console.log(JSON.stringify(authorizePagesDeployment({ event, env: process.env, checkoutSha }), null, 2));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
