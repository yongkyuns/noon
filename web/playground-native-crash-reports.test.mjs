import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { collectWebKitCrashReports } from "../scripts/playground-browser-support.mjs";

async function fixture(t) {
  const root = await mkdtemp(path.join(tmpdir(), "noon-native-crash-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const directory = path.join(root, "reports");
  const artifacts = path.join(root, "artifacts");
  await mkdir(directory); await mkdir(artifacts);
  const now = Date.now();
  const options = { directories: [directory], pids: [123], startedAtMs: now - 1000,
    endedAtMs: now + 1000, artifacts, prefix: "scene" };
  const put = async (name, changes = {}, metadata = { bug_type: "309" }) => {
    const report = { pid: 123, captureTime: new Date(now).toISOString().slice(0, -1).replace("T", " ") + "0 +0000",
      // Privacy redaction must not prevent matching the observed process ID.
      procPath: "/Users/USER/Library/Caches/*/WebContent.Development", ...changes };
    const bytes = JSON.stringify(metadata) + "\n" + JSON.stringify(report, null, 2);
    await writeFile(path.join(directory, name), bytes);
    return bytes;
  };
  return { ...options, directory, put, now };
}

test("native crash evidence retains only observed processes within the failed case", async t => {
  const f = await fixture(t);
  const bytes = await f.put("WebContent.Development-valid.ips", {
    exception: { type: "EXC_BAD_ACCESS" }, termination: { namespace: "SIGNAL", code: 11 },
  });
  await f.put("com.apple.WebKit.WebContent-other.ips", { pid: 456 });
  await f.put("com.apple.WebKit.WebContent-old.ips", { captureTime: new Date(f.now - 5000).toISOString() });
  await f.put("com.apple.WebKit.WebContent-future.ips", { captureTime: new Date(f.now + 5000).toISOString() });
  await f.put("com.apple.WebKit.WebContent-stackshot.ips", {}, { bug_type: "288" });
  await f.put("com.apple.WebKit.WebContent-malformed-time.ips", { captureTime: "invalid" });
  const result = await collectWebKitCrashReports(f);
  assert.equal(result.reports.length, 1);
  assert.equal(result.reports[0].pid, 123);
  assert.deepEqual(result.reports[0].exception, { type: "EXC_BAD_ACCESS" });
  assert.deepEqual(result.reports[0].termination, { namespace: "SIGNAL", code: 11 });
  assert.equal(await readFile(path.join(f.artifacts, result.reports[0].file), "utf8"), bytes);
});

test("crash evidence is bounded and ignores malformed, oversized and symlinked files", async t => {
  const f = await fixture(t);
  await writeFile(path.join(f.directory, "com.apple.WebKit-broken.ips"), "not json");
  await writeFile(path.join(f.directory, "com.apple.WebKit-huge.ips"), Buffer.alloc(2 * 1024 * 1024 + 1));
  await symlink("com.apple.WebKit-huge.ips", path.join(f.directory, "com.apple.WebKit-linked.ips"));
  assert.equal((await collectWebKitCrashReports(f)).reports.length, 0);
  for (let index = 0; index < 5; index++) await f.put(`com.apple.WebKit-${index}.ips`);
  assert.equal((await collectWebKitCrashReports(f)).reports.length, 3);
});

test("missing reports and unknown process ownership are explicit without masking the scene failure", async t => {
  const f = await fixture(t);
  assert.equal((await collectWebKitCrashReports({ ...f, pids: [] })).reason, "no observed WebKit process IDs");
  assert.equal((await collectWebKitCrashReports({ ...f, startedAtMs: Number.NaN })).reason, "invalid crash capture window");
  const missing = await collectWebKitCrashReports({ ...f, directories: [path.join(f.directory, "missing")] });
  assert.equal(missing.reports.length, 0);
  assert.equal(missing.errors[0].code, "ENOENT");
});
