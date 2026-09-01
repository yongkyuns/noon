import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const workflowPath = new URL("../.github/workflows/retained-text-animation.yml", import.meta.url);
const workflow = readFileSync(workflowPath, "utf8");

function triggerBlock(name) {
  const lines = workflow.split(/\r?\n/);
  const header = `  ${name}:`;
  const start = lines.findIndex((line) => line === header);
  assert.notEqual(start, -1, `retained Text workflow must define ${name}`);

  const block = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (/^  [A-Za-z_][A-Za-z0-9_-]*:/.test(line)) break;
    block.push(line);
  }
  return block;
}

function pathsForTrigger(name) {
  const block = triggerBlock(name);
  const pathsIndex = block.findIndex((line) => line === "    paths:");
  assert.notEqual(pathsIndex, -1, `${name} must define a retained Text path filter`);

  const paths = [];
  for (let index = pathsIndex + 1; index < block.length; index += 1) {
    const match = block[index].match(/^      - ["'](.+)["']$/);
    if (!match) break;
    paths.push(match[1]);
  }
  return paths;
}

test("retained Text regressions gate matching pull-request and master paths", () => {
  const pushPaths = pathsForTrigger("push");
  const pullRequestPaths = pathsForTrigger("pull_request");

  assert.ok(pushPaths.length > 0, "retained Text workflow must own at least one path");
  assert.deepEqual(
    pullRequestPaths,
    pushPaths,
    "pull requests and master pushes must exercise the same retained Text ownership boundary",
  );

  assert.ok(pushPaths.includes("scripts/retained-text-playback-smoke.mjs"));
  assert.ok(pushPaths.includes("web/python/_manim_animate.py"));
  assert.ok(pushPaths.includes(".github/workflows/retained-text-animation.yml"));
});

test("post-merge retained Text validation remains scoped to master", () => {
  const push = triggerBlock("push").join("\n");
  assert.match(push, /^    branches:\n      - master(?:\n|$)/m);
});
