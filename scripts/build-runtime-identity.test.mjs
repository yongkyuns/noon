import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  createRuntimeBuildIdentity,
  observedSourceRevision,
  RUNTIME_BUILD_IDENTITY_PATH,
  writeRuntimeBuildIdentity,
} from "./build-runtime-identity.mjs";

const digest = (value) => createHash("sha256").update(value).digest("hex");

async function fixture(t) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-runtime-build-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const put = async (name, value) => {
    const filename = path.join(root, name);
    await mkdir(path.dirname(filename), { recursive: true });
    await writeFile(filename, value);
  };
  await put(".gitignore", [
    "/web/pkg",
    "/web/python-worker.js",
    "/web/runtime-build-identity.json",
    "",
  ].join("\n"));
  await put("tracked.txt", "clean\n");
  await put("web/runtime-build-verifier.js", "export const verifier = true;\n");
  const git = (...args) => execFileSync("git", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
  git("init", "-q");
  git("add", ".gitignore", "tracked.txt", "web/runtime-build-verifier.js");
  git("-c", "user.name=Noon Test", "-c", "user.email=noon@example.invalid", "commit", "-qm", "fixture");

  await put("web/python-worker.js", "worker bytes\n");
  await put("web/pkg/noon_web_bg.wasm", Buffer.from([0, 97, 115, 109, 1]));
  await put("web/pkg/noon_web.js", "export default function init() {}\n");
  return { root, put, git };
}

test("identity binds exact runtime bytes without timestamps", async (t) => {
  const { root, git } = await fixture(t);
  const first = await createRuntimeBuildIdentity(root);
  const second = await createRuntimeBuildIdentity(root);
  assert.deepEqual(second, first);
  assert.equal(first.schema, 1);
  assert.equal(first.sourceRevision, git("rev-parse", "HEAD"));
  assert.equal(first.files.worker.path, "./python-worker.js");
  assert.equal(first.files.worker.sha256, digest("worker bytes\n"));
  assert.equal(first.files.wasm.sha256, digest(Buffer.from([0, 97, 115, 109, 1])));
  assert.equal(first.files.glue.sha256, digest("export default function init() {}\n"));
  assert.equal(first.files.verifier.sha256, digest("export const verifier = true;\n"));
  assert.match(first.buildId, /^[0-9a-f]{64}$/);

  const written = await writeRuntimeBuildIdentity(root);
  assert.deepEqual(JSON.parse(await readFile(path.join(root, RUNTIME_BUILD_IDENTITY_PATH), "utf8")), written);
});

test("changing actual runtime bytes changes the build identity", async (t) => {
  const { root, put } = await fixture(t);
  const original = await createRuntimeBuildIdentity(root);
  await put("web/python-worker.js", "different worker bytes\n");
  const changed = await createRuntimeBuildIdentity(root);
  assert.notEqual(changed.files.worker.sha256, original.files.worker.sha256);
  assert.notEqual(changed.buildId, original.buildId);
});

test("dirty or unavailable checkout provenance is explicit null, never guessed", async (t) => {
  const { root, put } = await fixture(t);
  await put("tracked.txt", "dirty\n");
  assert.equal(observedSourceRevision(root), null);
  assert.equal((await createRuntimeBuildIdentity(root)).sourceRevision, null);

  const notGit = await mkdtemp(path.join(os.tmpdir(), "noon-runtime-no-git-"));
  t.after(() => rm(notGit, { recursive: true, force: true }));
  assert.equal(observedSourceRevision(notGit), null);
});

test("missing runtime bytes fail instead of producing partial provenance", async (t) => {
  const { root } = await fixture(t);
  await rm(path.join(root, "web/pkg/noon_web_bg.wasm"));
  await assert.rejects(createRuntimeBuildIdentity(root), /noon_web_bg\.wasm|ENOENT/);
});
