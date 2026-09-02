import assert from "node:assert/strict";
import fs from "node:fs";

const workflowUrl = new URL(
  "../.github/workflows/manim-raster-differential.yml",
  import.meta.url,
);
const workflow = fs.readFileSync(workflowUrl, "utf8");
const lines = workflow.split(/\r?\n/);

function eventBlock(name) {
  const marker = `  ${name}:`;
  const start = lines.findIndex((line) => line === marker);
  assert.notEqual(start, -1, `missing ${name} workflow trigger`);

  let end = start + 1;
  while (end < lines.length) {
    const line = lines[end];
    if (/^  [A-Za-z_][A-Za-z0-9_-]*:\s*$/.test(line)) {
      break;
    }
    if (/^[A-Za-z_][A-Za-z0-9_-]*:\s*$/.test(line)) {
      break;
    }
    end += 1;
  }
  return lines.slice(start + 1, end);
}

function listValues(block, key) {
  const marker = `    ${key}:`;
  const start = block.findIndex((line) => line === marker);
  assert.notEqual(start, -1, `missing ${key} in workflow trigger`);

  const values = [];
  for (let index = start + 1; index < block.length; index += 1) {
    const line = block[index];
    if (/^    [A-Za-z_][A-Za-z0-9_-]*:\s*$/.test(line)) {
      break;
    }
    const match = line.match(/^      -\s+["']?(.+?)["']?\s*$/);
    if (match) {
      values.push(match[1]);
    }
  }
  return values;
}

const push = eventBlock("push");
const pullRequest = eventBlock("pull_request");

assert.deepEqual(
  listValues(pullRequest, "branches"),
  listValues(push, "branches"),
  "push and pull_request branch filters must stay identical",
);
assert.deepEqual(
  listValues(pullRequest, "paths-ignore"),
  listValues(push, "paths-ignore"),
  "push and pull_request path filters must stay identical",
);
assert.deepEqual(listValues(push, "branches"), ["master"]);
assert.deepEqual(listValues(push, "paths-ignore"), ["crates/**/tests/**"]);
assert.ok(
  lines.includes("  workflow_dispatch:"),
  "manual raster validation must remain available",
);

console.log("Manim raster workflow trigger policy is valid");
