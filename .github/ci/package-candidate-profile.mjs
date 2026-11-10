// Select one candidate run from the triggering event, never persistent PR labels.
import assert from "node:assert/strict";
import { appendFile, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ordinaryPullRequestActions = new Set(["opened", "reopened", "synchronize"]);
const fullMatrixLabel = "release-full-matrix";
const isObject = (value) => value !== null && typeof value === "object" && !Array.isArray(value);

export function selectPackageCandidateProfile(eventName, event) {
  assert.equal(typeof eventName, "string", "GITHUB_EVENT_NAME must be a string");
  assert.ok(eventName.length > 0, "GITHUB_EVENT_NAME is required");
  assert.ok(isObject(event), "GitHub event must be a JSON object");

  if (eventName === "workflow_dispatch") {
    return { run: true, profile: "full", wheelBuild: "cp3{11,12,13,14,15}-*" };
  }
  if (eventName === "pull_request") {
    assert.ok(isObject(event.pull_request), "pull_request event is missing its pull request");
    if (event.action === "labeled" && event.label?.name === fullMatrixLabel) {
      return { run: true, profile: "full", wheelBuild: "cp3{11,12,13,14,15}-*" };
    }
    if (ordinaryPullRequestActions.has(event.action)) {
      return { run: true, profile: "minimum", wheelBuild: "cp311-*" };
    }
  }
  return { run: false, profile: "ignored", wheelBuild: "" };
}

async function main() {
  assert.ok(process.env.GITHUB_EVENT_PATH, "GITHUB_EVENT_PATH is required");
  assert.ok(process.env.GITHUB_OUTPUT, "GITHUB_OUTPUT is required");
  const event = JSON.parse(await readFile(process.env.GITHUB_EVENT_PATH, "utf8"));
  const profile = selectPackageCandidateProfile(process.env.GITHUB_EVENT_NAME, event);
  // Values are constants selected above; no event text is interpolated into the
  // Actions output protocol or shell commands. Malformed input writes nothing.
  await appendFile(process.env.GITHUB_OUTPUT,
    `run=${profile.run}\nprofile=${profile.profile}\nwheel-build=${profile.wheelBuild}\n`);
  console.log(JSON.stringify(profile));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
