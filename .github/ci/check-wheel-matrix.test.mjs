import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./check-wheel-matrix.mjs", import.meta.url));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const platformTags = { linux: "manylinux_2_28_x86_64", macos: "macosx_12_0_arm64", windows: "win_amd64" };
const hostPlatform = { linux: "linux", darwin: "macos", win32: "windows" }[process.platform];

async function fixture(t) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "noon-wheel-matrix-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  return directory;
}

async function wheel(directory, interpreter, { platform = "linux", suffix = "", tags, override = {} } = {}) {
  const filename = `noon-0.1.0${suffix}-${interpreter}-${interpreter}-${platformTags[platform]}.whl`;
  // Matrix verification treats already-audited wheel archives as opaque bytes.
  // Real files and hashes exercise the filesystem/report binding independently
  // of the existing Python archive validator's ZIP/content contract tests.
  const bytes = Buffer.from(`audited native wheel fixture ${filename}\0\xff`, "utf8");
  const extension = `_noon_native.${interpreter}-${platform === "windows" ? "win_amd64.pyd" : "x86_64.so"}`;
  const audit = { wheel: { file: path.join(directory, filename), sha256: hash(bytes), version: "0.1.0",
    native_extension: extension, native_payloads: [{ path: extension, bytes: 24, sha256: hash(Buffer.from("native extension fixture")) }],
    tags: tags ?? [`${interpreter}-${interpreter}-${platformTags[platform]}`], ...override } };
  await writeFile(path.join(directory, filename), bytes);
  await writeFile(path.join(directory, `${filename}.report.json`), JSON.stringify(audit));
  return { filename, bytes, audit };
}

async function run(directory, { profile = "minimum", platform = "linux", extra = [], expected = 0 } = {}) {
  const args = [script, "--profile", profile, "--directory", directory];
  if (platform !== null) args.push("--platform", platform);
  const result = spawnSync(process.execPath, [...args, ...extra], { encoding: "utf8" });
  assert.equal(result.status, expected, result.stderr || result.stdout);
  const report = JSON.parse(result.stdout);
  assert.deepEqual(JSON.parse(await readFile(path.join(directory, "wheel-matrix-report.json"), "utf8")), report);
  assert.equal(report.status, expected ? "failed" : "passed");
  return report;
}

test("minimum binds the actual file hash and preserves multiple platform tags", async t => {
  const directory = await fixture(t);
  const tags = ["cp311-cp311-manylinux_2_28_x86_64", "cp311-cp311-manylinux_2_17_x86_64.manylinux2014_x86_64"];
  const item = await wheel(directory, "cp311", { tags });
  const report = await run(directory);
  assert.deepEqual(report.wheels[0].tags, tags);
  assert.equal(report.wheels[0].sha256, hash(item.bytes));
  assert.deepEqual(report.cells[0].validFiles, [item.filename]);
  assert.deepEqual(await readFile(path.join(directory, item.filename)), item.bytes);
});

test("full requires all five versions and defaults to the current host platform", async t => {
  const directory = await fixture(t);
  for (const interpreter of ["cp311", "cp312", "cp313", "cp314", "cp315"]) await wheel(directory, interpreter, { platform: hostPlatform });
  const report = await run(directory, { profile: "full", platform: null });
  assert.equal(report.platform, hostPlatform);
  assert.equal(report.cells.length, 5);
  assert.ok(report.cells.every(cell => cell.status === "passed"));
});

test("missing full-matrix cell fails and retains evidence", async t => {
  const directory = await fixture(t);
  for (const interpreter of ["cp311", "cp312", "cp313", "cp314"]) await wheel(directory, interpreter);
  const report = await run(directory, { profile: "full", expected: 1 });
  assert.ok(report.errors.some(error => error.includes("cp315")));
  assert.deepEqual(report.cells.find(cell => cell.interpreter === "cp315").files, []);
});

test("two distinct wheel files cannot fill one interpreter cell", async t => {
  const directory = await fixture(t);
  await wheel(directory, "cp311");
  await wheel(directory, "cp311", { suffix: "-1" });
  const report = await run(directory, { expected: 1 });
  assert.equal(report.cells[0].files.length, 2);
  assert.match(report.errors.join("\n"), /exactly one valid cp311/);
});

test("free-threaded, abi3, pure, mismatched and unexpected interpreter tags fail", async t => {
  for (const tag of ["cp313-cp313t-manylinux_2_28_x86_64", "cp311-abi3-manylinux_2_28_x86_64",
    "py3-none-any", "cp311-cp312-manylinux_2_28_x86_64", "cp316-cp316-manylinux_2_28_x86_64",
    "cp311-cp311-any", "cp311.cp312-cp311-manylinux_2_28_x86_64"]) {
    await t.test(tag, async t => {
      const directory = await fixture(t);
      await wheel(directory, "cp311", { tags: [tag] });
      const report = await run(directory, { expected: 1 });
      assert.ok(report.wheels[0].errors.length);
    });
  }
});

test("extra stale wheel, missing audit and orphan audit cannot pass", async t => {
  for (const mode of ["extra", "missing-report", "orphan-report"]) {
    await t.test(mode, async t => {
      const directory = await fixture(t);
      const item = await wheel(directory, "cp311");
      if (mode === "extra") await wheel(directory, "cp312");
      if (mode === "missing-report") await rm(path.join(directory, `${item.filename}.report.json`));
      if (mode === "orphan-report") await rm(path.join(directory, item.filename));
      const report = await run(directory, { expected: 1 });
      assert.ok(report.errors.length || report.wheels.some(item => item.errors.length));
    });
  }
});

test("changing wheel bytes after its audit is rejected", async t => {
  const directory = await fixture(t);
  const item = await wheel(directory, "cp311");
  await writeFile(path.join(directory, item.filename), Buffer.from("different artifact"));
  const report = await run(directory, { expected: 1 });
  assert.match(report.wheels[0].errors.join("\n"), /do not match the audit SHA256/);
});

test("report basename must match; Windows-origin report paths remain portable", async t => {
  const directory = await fixture(t);
  const item = await wheel(directory, "cp311", { platform: "windows" });
  item.audit.wheel.file = `C:\\build\\wheelhouse\\${item.filename}`;
  await writeFile(path.join(directory, `${item.filename}.report.json`), JSON.stringify(item.audit));
  await run(directory, { platform: "windows" });
  item.audit.wheel.file = "C:\\build\\other.whl";
  await writeFile(path.join(directory, `${item.filename}.report.json`), JSON.stringify(item.audit));
  const report = await run(directory, { platform: "windows", expected: 1 });
  assert.match(report.wheels[0].errors.join("\n"), /basename/);
});

test("wrong platform, mixed interpreters and duplicate tag rows are rejected", async t => {
  for (const tags of [["cp311-cp311-macosx_12_0_arm64"],
    ["cp311-cp311-manylinux_2_28_x86_64", "cp312-cp312-manylinux_2_28_x86_64"],
    ["cp311-cp311-manylinux_2_28_x86_64", "cp311-cp311-manylinux_2_28_x86_64"]]) {
    await t.test(tags.join(","), async t => {
      const directory = await fixture(t);
      await wheel(directory, "cp311", { tags });
      await run(directory, { expected: 1 });
    });
  }
});

test("native payload and readable complete audit metadata are required", async t => {
  for (const override of [{ native_extension: null }, { native_payloads: [] }, { tags: [] }, { sha256: "not-a-digest" }]) {
    await t.test(JSON.stringify(override), async t => {
      const directory = await fixture(t);
      await wheel(directory, "cp311", { override });
      await run(directory, { expected: 1 });
    });
  }
  const directory = await fixture(t);
  const item = await wheel(directory, "cp311");
  await writeFile(path.join(directory, `${item.filename}.report.json`), "{broken JSON");
  const report = await run(directory, { expected: 1 });
  assert.match(report.wheels[0].errors.join("\n"), /audit read failed/);
});

test("invalid profile and absent directory still emit failure evidence", async t => {
  const parent = await fixture(t);
  await run(path.join(parent, "not-created"), { expected: 1 });
  const report = await run(parent, { profile: "all", expected: 1 });
  assert.match(report.errors.join("\n"), /profile must be minimum or full/);
});
