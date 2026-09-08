import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { buildScrubbedEnvironment, runBoundedChild } from "./agent-preview-process.mjs";

async function withTempDir(run) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "noon-agent-preview-"));
  try { return await run(directory); }
  finally { await rm(directory, { recursive: true, force: true }); }
}

test("scrubbed environment does not inherit arbitrary parent variables", () => {
  const previous = process.env.NOON_AGENT_PREVIEW_SECRET_TEST;
  process.env.NOON_AGENT_PREVIEW_SECRET_TEST = "secret";
  try {
    const environment = buildScrubbedEnvironment({ NOON_AGENT_PREVIEW_ALLOWED: "yes" });
    assert.equal(environment.NOON_AGENT_PREVIEW_SECRET_TEST, undefined);
    assert.equal(environment.NOON_AGENT_PREVIEW_ALLOWED, "yes");
    assert.equal(Object.isFrozen(environment), true);
  } finally {
    if (previous === undefined) delete process.env.NOON_AGENT_PREVIEW_SECRET_TEST;
    else process.env.NOON_AGENT_PREVIEW_SECRET_TEST = previous;
  }
});

test("bounded child reports output and exit status", async () => {
  await withTempDir(async (cwd) => {
    const result = await runBoundedChild({
      command: process.execPath,
      args: ["-e", "console.log(process.env.NOON_AGENT_PREVIEW_ALLOWED); console.error('diagnostic')"],
      cwd,
      env: { NOON_AGENT_PREVIEW_ALLOWED: "visible" },
      timeoutMs: 2_000,
    });
    assert.equal(result.exitCode, 0);
    assert.equal(result.signal, null);
    assert.equal(result.terminationReason, null);
    assert.equal(result.stdout, "visible\n");
    assert.equal(result.stderr, "diagnostic\n");
  });
});

test("bounded child terminates a hung process group on deadline", async () => {
  await withTempDir(async (cwd) => {
    const result = await runBoundedChild({
      command: process.execPath,
      args: ["-e", "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000)"],
      cwd,
      timeoutMs: 80,
      killGraceMs: 80,
    });
    assert.equal(result.terminationReason, "timeout");
    assert.equal(result.forcedKill, true);
    assert.ok(result.signal === "SIGKILL" || result.signal === "SIGTERM");
    assert.ok(result.durationMs < 2_000);
  });
});

test("bounded child terminates when output exceeds the configured budget", async () => {
  await withTempDir(async (cwd) => {
    const result = await runBoundedChild({
      command: process.execPath,
      args: ["-e", "process.stdout.write('x'.repeat(4096)); setInterval(() => {}, 1000)"],
      cwd,
      timeoutMs: 2_000,
      maxOutputBytes: 128,
      killGraceMs: 80,
    });
    assert.equal(result.terminationReason, "output_limit");
    assert.ok(result.stdout.length <= 128);
    assert.ok(result.durationMs < 2_000);
  });
});

test("invalid environment names and values fail before spawn", async () => {
  assert.throws(() => buildScrubbedEnvironment({ "bad-name": "x" }), /invalid environment key/);
  assert.throws(() => buildScrubbedEnvironment({ BAD: 3 }), /must be a string/);
});
