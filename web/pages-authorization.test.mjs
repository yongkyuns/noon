import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { authorizePagesDeployment } from "../.github/ci/pages-authorization.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const helper = path.join(root, ".github/ci/pages-authorization.mjs");
const sha = "a".repeat(40);
const other = "b".repeat(40);
function request() {
  return { event: { ref: "master", repository: { full_name: "yongkyuns/noon" },
    inputs: { expected_sha: sha, confirm_deploy: "true" } },
  env: { GITHUB_EVENT_NAME: "workflow_dispatch", GITHUB_REPOSITORY: "yongkyuns/noon",
    GITHUB_REF: "refs/heads/master", GITHUB_SHA: sha }, checkoutSha: sha };
}

test("only a confirmed exact-source manual request authorizes the demo", () => {
  for (const confirmation of [true, "true"]) {
    for (const ref of ["master", "refs/heads/master"]) {
      const input = request();
      input.event.inputs.confirm_deploy = confirmation;
      input.event.ref = ref;
      assert.deepEqual(authorizePagesDeployment(input), { schema: 1, repository: "yongkyuns/noon",
        ref: "refs/heads/master", sourceRevision: sha,
        authorization: "explicit-demo-deployment", releaseApproval: false });
    }
  }
});

for (const eventName of [undefined, "push", "pull_request", "pull_request_target",
  "workflow_run", "repository_dispatch", "schedule", "release"]) {
  test(`Pages rejects ${String(eventName)} even with valid-looking inputs`, () => {
    const input = request(); input.env.GITHUB_EVENT_NAME = eventName;
    assert.throws(() => authorizePagesDeployment(input), /explicit manual dispatch/);
  });
}

for (const confirmation of [undefined, null, false, "false", "", "TRUE", "yes", 1, "1", {}]) {
  test(`Pages rejects non-confirmation ${JSON.stringify(confirmation)}`, () => {
    const input = request(); input.event.inputs.confirm_deploy = confirmation;
    assert.throws(() => authorizePagesDeployment(input), /not explicitly confirmed/);
  });
}

test("forks, other branches, tags and contradictory payloads cannot authorize", () => {
  for (const [key, value] of [["GITHUB_REPOSITORY", "other/noon"],
    ["GITHUB_REF", "refs/heads/topic"], ["GITHUB_REF", "refs/tags/master"],
    ["GITHUB_REF", "refs/heads/master-extra"], ["GITHUB_REF", "master"]]) {
    const input = request(); input.env[key] = value;
    assert.throws(() => authorizePagesDeployment(input));
  }
  for (const edit of [input => { input.event.repository.full_name = "other/noon"; },
    input => { input.event.ref = "refs/tags/master"; },
    input => { input.event.ref = "topic"; },
    input => { delete input.event.repository; },
    input => { delete input.event.inputs; },
    input => { input.event = null; }]) {
    const input = request(); edit(input);
    assert.throws(() => authorizePagesDeployment(input));
  }
});

test("abbreviated, mutable, malformed and different revisions fail closed", () => {
  for (const expected of [undefined, null, 1, "master", "HEAD", "a".repeat(7),
    "A".repeat(40), "g".repeat(40), `${sha}\n`, ` ${sha}`, `${sha};echo unsafe`, other]) {
    const input = request(); input.event.inputs.expected_sha = expected;
    assert.throws(() => authorizePagesDeployment(input));
  }
  for (const edit of [input => { input.checkoutSha = other; },
    input => { input.env.GITHUB_SHA = other; },
    input => { delete input.env.GITHUB_SHA; }]) {
    const input = request(); edit(input);
    assert.throws(() => authorizePagesDeployment(input));
  }
});

function section(source, name) {
  const lines = source.split("\n");
  const start = lines.indexOf(`${name}:`);
  assert.ok(start >= 0, `${name} section is required`);
  const end = lines.findIndex((line, index) => index > start && /^[a-zA-Z][\w-]*:/.test(line));
  return lines.slice(start + 1, end < 0 ? undefined : end)
    .filter(line => line.trim() && !line.trimStart().startsWith("#")).join("\n");
}

function assertWorkflow(source) {
  const events = section(source, "on");
  assert.deepEqual(events.split("\n").filter(line => /^  \S/.test(line)), ["  workflow_dispatch:"],
    "no automatic event may select the Pages workflow");
  assert.match(events, /expected_sha:\n\s+description: [^\n]+\n\s+required: true\n\s+type: string/);
  assert.match(events, /confirm_deploy:\n\s+description: [^\n]+\n\s+required: true\n\s+default: false\n\s+type: boolean/);
  assert.equal(section(source, "permissions"), "  contents: read");
  assert.deepEqual(section(source, "jobs").split("\n").filter(line => /^  \S/.test(line)), ["  build:"],
    "a new Pages job needs an explicit authorization review");
  assert.match(source, /^    if: \$\{\{ github\.event_name == 'workflow_dispatch' && github\.repository == 'yongkyuns\/noon' && github\.ref == 'refs\/heads\/master' && inputs\.confirm_deploy == true && inputs\.expected_sha == github\.sha \}\}$/m);
  assert.match(source, /^    permissions:\n      contents: read\n      pages: write\n      id-token: write$/m);
  assert.match(source, /environment:\n      name: github-pages/);
  assert.match(source, /ref: \$\{\{ github\.sha \}\}\n          persist-credentials: false/);
  const authorize = source.indexOf("      - name: Authorize exact demo revision\n        run: node .github/ci/pages-authorization.mjs");
  const build = source.indexOf("      - name: Build and validate playground\n        run: bash scripts/build-web-demo.sh");
  const recheck = source.indexOf("      - name: Recheck deployment authorization\n        run: node .github/ci/pages-authorization.mjs");
  const configure = source.indexOf("      - name: Configure Pages");
  const upload = source.indexOf("      - name: Upload Pages artifact");
  const deploy = source.indexOf("      - name: Deploy\n");
  const verify = source.indexOf("      - name: Verify deployed revision");
  assert.ok(authorize > 0 && build > authorize && recheck > build && configure > recheck &&
    upload > configure && deploy > upload && verify > deploy, "authorization/build/publication order differs");
  assert.doesNotMatch(source, /continue-on-error:|if:.*(?:always\(|failure\(|cancelled\()/);
  assert.match(source, /NOON_WASM_PROFILE: release/);
  assert.match(source, /NOON_WASM_SKIP_OPT: "0"/);
  assert.match(source, /NOON_RENDERER_SMOKE: "0"/);
  assert.match(source, /include-hidden-files: true/);
  assert.match(source, /retention-days: 1/);
  assert.match(source, /EXPECTED_SHA: \$\{\{ github\.sha \}\}/);
}

test("real Pages workflow keeps authorization separate and preserves its checks", async () => {
  const source = await readFile(path.join(root, ".github/workflows/pages.yml"), "utf8");
  assertWorkflow(source);
  for (const [from, to] of [
    ["on:\n", "on:\n  push:\n    branches: [master]\n"],
    ["  workflow_dispatch:\n", "  workflow_run:\n"],
    ["        default: false", "        default: true"],
    ["inputs.confirm_deploy == true", "true"],
    ["inputs.expected_sha == github.sha", "true"],
    ["github.ref == 'refs/heads/master'", "true"],
    ["permissions:\n  contents: read", "permissions:\n  contents: read\n  pages: write"],
    ["        run: node .github/ci/pages-authorization.mjs", "        run: echo skipped"],
    ["        run: bash scripts/build-web-demo.sh", "        run: echo skipped"],
    ["      - name: Deploy\n", "      - name: Deploy\n        if: always()\n"],
    ["persist-credentials: false", "persist-credentials: true"],
  ]) {
    assert.ok(source.includes(from), `invalid negative control: ${from}`);
    assert.throws(() => assertWorkflow(source.replace(from, to)), `missed policy regression: ${from}`);
  }
});

test("real CLI accepts only the checked-out commit and never executes input", async () => {
  const temp = await mkdtemp(path.join(os.tmpdir(), "noon-pages-auth-"));
  try {
    const git = (...args) => execFileSync("git", args, { cwd: temp, encoding: "utf8" }).trim();
    git("init", "--quiet");
    git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "fixture");
    const checkedOut = git("rev-parse", "HEAD");
    const input = request();
    input.env.GITHUB_SHA = checkedOut;
    input.event.inputs.expected_sha = checkedOut;
    const eventPath = path.join(temp, "event.json");
    const run = () => spawnSync(process.execPath, [helper], { cwd: temp, encoding: "utf8",
      env: { ...process.env, ...input.env, GITHUB_EVENT_PATH: eventPath } });
    await writeFile(eventPath, JSON.stringify(input.event));
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    assert.equal(JSON.parse(result.stdout).sourceRevision, checkedOut);
    assert.equal(JSON.parse(result.stdout).releaseApproval, false);
    git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "--quiet", "--allow-empty", "-m", "movement");
    result = run();
    assert.equal(result.status, 1);
    assert.match(result.stderr, /checkout differs/);
    input.event.inputs.expected_sha = "$(touch SENTINEL)";
    await writeFile(eventPath, JSON.stringify(input.event));
    result = run();
    assert.equal(result.status, 1);
    await assert.rejects(readFile(path.join(temp, "SENTINEL")), { code: "ENOENT" });
    await writeFile(eventPath, "not json");
    assert.equal(run().status, 1);
    await rm(eventPath);
    assert.equal(run().status, 1);
  } finally { await rm(temp, { recursive: true, force: true }); }
});
