import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const rasterWorkflow = fs.readFileSync(
  new URL(
    "../.github/workflows/manim-raster-differential.yml",
    import.meta.url,
  ),
  "utf8",
);
const prFastWorkflow = fs.readFileSync(
  new URL("../.github/workflows/pr-fast.yml", import.meta.url),
  "utf8",
);
const lines = rasterWorkflow.split(/\r?\n/);

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

test("raster workflow keeps push validation scoped to master", () => {
  const push = eventBlock("push");

  assert.deepEqual(listValues(push, "branches"), ["master"]);
  assert.deepEqual(listValues(push, "paths-ignore"), ["crates/**/tests/**"]);
  assert.ok(
    lines.includes("  workflow_dispatch:"),
    "manual raster validation must remain available",
  );
});

test("pull requests always create the required raster gate", () => {
  const pullRequest = eventBlock("pull_request");

  assert.deepEqual(listValues(pullRequest, "branches"), ["master"]);
  assert.equal(
    pullRequest.includes("    paths-ignore:"),
    false,
    "required pull_request workflow must not be path-skipped",
  );
  assert.equal(
    pullRequest.includes("    paths:"),
    false,
    "required pull_request workflow must not be path-filtered",
  );
});

test("raster execution is scoped behind an always-running gate", () => {
  assert.match(rasterWorkflow, /name: Decide canonical raster gate/);
  assert.match(rasterWorkflow, /name: Canonical Manim raster differential/);
  assert.match(rasterWorkflow, /needs: scope/);
  assert.match(rasterWorkflow, /if: needs\.scope\.outputs\.run == 'true'/);
  assert.match(rasterWorkflow, /name: ManimCE 0\.21\.0 raster oracle/);
  assert.match(rasterWorkflow, /if: always\(\)/);
  assert.match(rasterWorkflow, /needs:\n      - scope\n      - raster/);
});

test("workflow policy validation runs in the always-on PR Fast Gate", () => {
  assert.match(
    prFastWorkflow,
    /run: node scripts\/manim-raster-workflow-policy\.test\.mjs/,
  );
});

console.log("Manim raster workflow trigger policy is valid");
