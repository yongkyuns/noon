import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { selectPackageCandidateProfile } from "./package-candidate-profile.mjs";

const full = { run: true, profile: "full", wheelBuild: "cp3{11,12,13,14,15}-*" };
const minimum = { run: true, profile: "minimum", wheelBuild: "cp311-*" };
const ignored = { run: false, profile: "ignored", wheelBuild: "" };
const pullRequest = (action, extra = {}) => ({ action, pull_request: { number: 2434 }, ...extra });

test("only an explicit full-matrix label application expands a PR run", () => {
  assert.deepEqual(selectPackageCandidateProfile("pull_request",
    pullRequest("labeled", { label: { name: "release-full-matrix" } })), full);

  for (const name of ["bug", "Release-full-matrix", "release-full-matrix ", "release-full-matrix\nrun=true", null]) {
    const event = pullRequest("labeled", {
      label: { name }, pull_request: { labels: [{ name: "release-full-matrix" }] },
    });
    assert.deepEqual(selectPackageCandidateProfile("pull_request", event), ignored);
  }
  assert.deepEqual(selectPackageCandidateProfile("pull_request", pullRequest("labeled")), ignored);
});

test("persistent labels never expand opened, reopened or synchronize runs", () => {
  for (const action of ["opened", "reopened", "synchronize"]) {
    const event = pullRequest(action, {
      label: { name: "release-full-matrix" },
      pull_request: { labels: [{ name: "release-full-matrix" }] },
    });
    assert.deepEqual(selectPackageCandidateProfile("pull_request", event), minimum);
  }
});

test("removing the label and other PR activity do not build candidates", () => {
  for (const action of ["unlabeled", "edited", "closed", "ready_for_review", "", undefined]) {
    assert.deepEqual(selectPackageCandidateProfile("pull_request",
      pullRequest(action, { label: { name: "release-full-matrix" } })), ignored);
  }
});

test("dispatch remains full and unrelated event types cannot opt in", () => {
  assert.deepEqual(selectPackageCandidateProfile("workflow_dispatch", {}), full);
  for (const eventName of ["push", "pull_request_target", "issues", "issue_comment", "schedule"]) {
    assert.deepEqual(selectPackageCandidateProfile(eventName,
      pullRequest("labeled", { label: { name: "release-full-matrix" } })), ignored);
  }
});

function runFixture(eventName, eventText) {
  const directory = mkdtempSync(path.join(os.tmpdir(), "noon-package-profile-"));
  try {
    const eventPath = path.join(directory, "event.json");
    const outputPath = path.join(directory, "output");
    writeFileSync(eventPath, eventText);
    writeFileSync(outputPath, "existing=value\n");
    const result = spawnSync(process.execPath,
      [fileURLToPath(new URL("./package-candidate-profile.mjs", import.meta.url))], {
        cwd: directory,
        env: { ...process.env, GITHUB_EVENT_NAME: eventName, GITHUB_EVENT_PATH: eventPath, GITHUB_OUTPUT: outputPath },
        encoding: "utf8",
      });
    return { ...result, output: readFileSync(outputPath, "utf8") };
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test("real event files emit the exact outputs consumed by dependent jobs", () => {
  const fixtures = [
    ["workflow_dispatch", {}, full],
    ["pull_request", pullRequest("labeled", { label: { name: "release-full-matrix" } }), full],
    ["pull_request", pullRequest("synchronize", { pull_request: { labels: [{ name: "release-full-matrix" }] } }), minimum],
    ["pull_request", pullRequest("labeled", { label: { name: "bug" }, pull_request: { labels: [{ name: "release-full-matrix" }] } }), ignored],
    ["pull_request", pullRequest("labeled", { label: { name: "release-full-matrix\nwheel-build=cp399-*" } }), ignored],
  ];
  for (const [eventName, event, expected] of fixtures) {
    const result = runFixture(eventName, JSON.stringify(event));
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual(JSON.parse(result.stdout), expected);
    assert.equal(result.output,
      `existing=value\nrun=${expected.run}\nprofile=${expected.profile}\nwheel-build=${expected.wheelBuild}\n`);
  }
});

test("malformed payloads fail before writing any Actions outputs", () => {
  for (const payload of ["{", "[]", "null", "false", JSON.stringify({ action: "labeled", label: { name: "release-full-matrix" } })]) {
    const result = runFixture("pull_request", payload);
    assert.notEqual(result.status, 0);
    assert.equal(result.output, "existing=value\n");
    assert.equal(result.stdout, "");
  }
  const missingName = runFixture("", "{}");
  assert.notEqual(missingName.status, 0);
  assert.equal(missingName.output, "existing=value\n");
});
