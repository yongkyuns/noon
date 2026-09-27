// Exercise the actual CI shell steps; substitutes below model process/I/O failures,
// not Manim, browser rendering, or the correctness of any fixture pixels.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

const workflow = await readFile(new URL(
  "../.github/workflows/foreground-matching-qualification.yml", import.meta.url), "utf8");
const stages = [
  ["raster", "Capture pinned Manim and real Python-worker overlap frames", "scripts/manim-raster-differential.mjs"],
  ["tolerances", "Enforce whole-frame raster tolerances", "scripts/manim-raster-enforce.mjs"],
  ["witnesses", "Prove actual overlap order and visible target occurrences", "scripts/foreground-matching-checks.mjs"],
  ["cadence", "Prove sparse and dense forward playback agree exactly", "scripts/shared-playback-raster.mjs"],
];

function step(name) {
  const text = workflow.split(`      - name: ${name}\n`)[1]?.split("\n      - ")[0];
  assert.ok(text, `missing CI step: ${name}`);
  return text;
}

function command(name) {
  const body = step(name).split("        run: |\n")[1];
  assert.ok(body, `missing literal run block: ${name}`);
  return body.split("\n").map(line => {
    assert.ok(!line || line.startsWith("          "), `unexpected shell indentation: ${name}`);
    return line.slice(10);
  }).join("\n");
}

async function sandbox(fn) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-foreground-evidence-"));
  try {
    const bin = path.join(root, "bin");
    await mkdir(bin);
    const fakeNode = path.join(bin, "node");
    await writeFile(fakeNode, `#!/bin/sh
printf 'command: %s\\n' "$*"
printf 'reference capture diagnostic\\n' >&2
# Emulate a child command replacing its own image-output directory.
rm -rf foreground-matching-artifacts
exit "\${TEST_EXIT:-0}"
`);
    await chmod(fakeNode, 0o755);
    await fn(root, { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH}` });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

for (const [id, name, script] of stages) {
  test(`${id} retains both streams and preserves success or the original failure exit`, async () => {
    assert.match(step(name), /^        continue-on-error: true$/m);
    assert.match(step(name), /^        shell: bash$/m);
    await sandbox(async (root, env) => {
      for (const status of [0, 7]) {
        const result = spawnSync("bash", ["-e", "-c", command(name)], {
          cwd: root, env: { ...env, TEST_EXIT: String(status) }, encoding: "utf8",
        });
        assert.equal(result.status, status, result.stderr);
        const log = await readFile(path.join(root, "ci-artifacts/foreground-matching", `${id}.log`), "utf8");
        assert.equal(log, `command: ${script}\nreference capture diagnostic\n`);
        assert.match(result.stdout, /reference capture diagnostic/);
      }
    });
  });
}

test("capture does not turn a logfile write failure into a successful verdict", async () => {
  await sandbox(async (root, env) => {
    await mkdir(path.join(root, "ci-artifacts/foreground-matching/raster.log"), { recursive: true });
    const result = spawnSync("bash", ["-e", "-c", command(stages[0][1])], {
      cwd: root, env: { ...env, TEST_EXIT: "0" }, encoding: "utf8",
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /tee:/);
  });
});

test("raw outcomes are written before unconditional evidence upload", async () => {
  const name = "Record qualification outcomes";
  assert.match(step(name), /^        if: always\(\)$/m);
  const upload = "Retain source, reference, pixels and strict verdicts";
  assert.match(step(upload), /^        if: always\(\)$/m);
  assert.ok(workflow.indexOf(name) < workflow.indexOf(upload));
  for (const [id] of stages) assert.ok(step(name).includes(`steps.${id}.outcome`));
  await sandbox(async (root, env) => {
    const result = spawnSync("bash", ["-e", "-c", command(name)], {
      cwd: root,
      env: { ...env, RASTER: "failure", TOLERANCES: "skipped", WITNESSES: "cancelled", CADENCE: "" },
      encoding: "utf8",
    });
    assert.equal(result.status, 0, result.stderr);
    assert.equal(await readFile(path.join(root, "ci-artifacts/foreground-matching/outcomes.txt"), "utf8"),
      "raster=failure\ntolerances=skipped\nwitnesses=cancelled\ncadence=\n");
  });
});

test("diagnostic retention never substitutes for any of the four required results", () => {
  const gate = command("Require every qualification stage");
  const green = Object.fromEntries(stages.map(([id]) => [id.toUpperCase(), "success"]));
  const execute = outcomes => spawnSync("bash", ["-e", "-c", gate], {
    env: { ...process.env, ...outcomes }, encoding: "utf8",
  });
  assert.equal(execute(green).status, 0);
  for (const key of Object.keys(green)) for (const value of ["", "failure", "skipped", "cancelled"]) {
    assert.notEqual(execute({ ...green, [key]: value }).status, 0, `${key}=${value}`);
  }
});
