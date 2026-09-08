import assert from "node:assert/strict";
import { getEventListeners } from "node:events";
import { mkdtemp, readFile, writeFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { buildScrubbedEnvironment, runBoundedChild } from "./agent-preview-process.mjs";

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const linux = { skip: process.platform !== "linux" };

async function withTempDir(run) {
  const directory = await mkdtemp(path.join(os.tmpdir(), "noon-agent-preview-"));
  try { return await run(directory); }
  finally { await rm(directory, { recursive: true, force: true }); }
}

async function within(work, ms = 3_000) {
  let timer;
  try {
    return await Promise.race([
      work,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error("supervisor did not settle within bound")), ms);
      }),
    ]);
  } finally { clearTimeout(timer); }
}

async function waitUntil(predicate) {
  const end = Date.now() + 3_000;
  while (Date.now() < end) {
    if (await predicate()) return;
    await delay(10);
  }
  assert.fail("fixture did not reach expected state");
}

async function readPid(cwd, name) {
  try { return Number(await readFile(path.join(cwd, `${name}.pid`), "utf8")); }
  catch (error) { if (error.code === "ENOENT") return null; throw error; }
}

// kill(pid, 0) counts unreaped orphans as present. Linux /proc distinguishes
// executing descendants from zombies; reaping is not a promise of this helper.
async function running(pid) {
  if (!pid) return false;
  try {
    const stat = await readFile(`/proc/${pid}/stat`, "utf8");
    return !["Z", "X"].includes(stat.slice(stat.lastIndexOf(") ") + 2)[0]);
  } catch (error) { if (error.code === "ENOENT") return false; throw error; }
}

async function fixture(cwd, { stdio = "inherit", exit = false, flood = false, escaped = false } = {}) {
  await writeFile(path.join(cwd, "descendant.cjs"), `
    const fs = require('node:fs');
    process.on('SIGTERM', () => {});
    fs.writeFileSync('descendant.pid', String(process.pid));
    setInterval(() => {}, 1000);
  `);
  await writeFile(path.join(cwd, "leader.cjs"), `
    const fs = require('node:fs');
    const { spawn } = require('node:child_process');
    fs.writeFileSync('leader.pid', String(process.pid));
    process.on('SIGTERM', () => process.exit(0));
    const child = spawn(process.execPath, ['descendant.cjs'], {
      stdio: ${JSON.stringify(stdio)}, detached: ${escaped}
    });
    child.unref();
    const timer = setInterval(() => {
      if (!fs.existsSync('descendant.pid')) return;
      clearInterval(timer);
      fs.writeFileSync('ready', 'yes');
      ${exit ? "process.exit(0);" : flood ? "setInterval(() => { process.stdout.write('x'.repeat(8192)); process.stderr.write('y'.repeat(8192)); }, 1);" : "setInterval(() => {}, 1000);"}
    }, 10);
  `);
}

async function withFixture(options, run) {
  return withTempDir(async (cwd) => {
    await fixture(cwd, options);
    let work;
    const start = (extra = {}) => {
      work = runBoundedChild({
        command: process.execPath, args: ["leader.cjs"], cwd,
        timeoutMs: 5_000, killGraceMs: 80, cleanupTimeoutMs: 300, ...extra,
      });
      // The harness always owns emergency cleanup, including red regressions.
      work.catch(() => {});
      return work;
    };
    try { return await run({ cwd, start }); }
    finally {
      const leader = await readPid(cwd, "leader");
      const descendant = await readPid(cwd, "descendant");
      for (const pid of [leader ? -leader : null, descendant]) {
        if (!pid) continue;
        try { process.kill(pid, "SIGKILL"); }
        catch (error) { if (error.code !== "ESRCH") throw error; }
      }
      if (work) await within(work.catch(() => {}));
    }
  });
}

async function ready(cwd) {
  await waitUntil(async () => {
    try { await readFile(path.join(cwd, "ready")); return true; }
    catch (error) { if (error.code === "ENOENT") return false; throw error; }
  });
}

async function assertStopped(cwd) {
  const leader = await readPid(cwd, "leader");
  const descendant = await readPid(cwd, "descendant");
  assert.ok(leader && descendant, "the regression must actually start two processes");
  // SIGKILL delivery is asynchronous; inspect before the harness's finally cleanup.
  await waitUntil(async () => !(await running(leader)) && !(await running(descendant)));
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

test("bounded child reports output, exit status and group cleanup", linux, async () => {
  await withTempDir(async (cwd) => {
    const result = await runBoundedChild({
      command: process.execPath,
      args: ["-e", "console.log(process.env.NOON_AGENT_PREVIEW_ALLOWED); console.error('diagnostic')"],
      cwd, env: { NOON_AGENT_PREVIEW_ALLOWED: "visible" }, timeoutMs: 2_000,
    });
    assert.equal(result.exitCode, 0);
    assert.equal(result.signal, null);
    assert.equal(result.terminationReason, null);
    assert.equal(result.stdout, "visible\n");
    assert.equal(result.stderr, "diagnostic\n");
    assert.equal(result.cleanup.outcome, "group_absent");
    assert.equal(result.cleanup.stdioClosed, true);
    assert.equal(Object.isFrozen(result.cleanup), true);
  });
});

for (const stdio of ["inherit", "ignore"]) {
  test(`exited leader cannot abandon descendant with ${stdio} stdio`, linux, async () => {
    await withFixture({ stdio, exit: true }, async ({ cwd, start }) => {
      const result = await within(start());
      await assertStopped(cwd);
      assert.equal(result.exitCode, 0);
      assert.equal(result.terminationReason, "descendant_cleanup");
      assert.equal(result.forcedKill, true);
      assert.equal(result.cleanup.stdioClosed, true);
      assert.ok(["sigkill_sent", "group_absent"].includes(result.cleanup.outcome));
    });
  });
}

test("deadline escalates after the leader exits on SIGTERM", linux, async () => {
  await withFixture({}, async ({ cwd, start }) => {
    const result = await within(start({ timeoutMs: 800 }));
    await assertStopped(cwd);
    assert.equal(result.terminationReason, "timeout");
    assert.equal(result.forcedKill, true);
  });
});

test("output flood has one bounded cleanup even when the leader exits", linux, async () => {
  await withFixture({ flood: true }, async ({ cwd, start }) => {
    const result = await within(start({ maxOutputBytes: 128, timeoutMs: 800 }));
    await assertStopped(cwd);
    assert.equal(result.terminationReason, "output_limit");
    assert.equal(result.forcedKill, true);
    assert.ok(Buffer.byteLength(result.stdout) <= 128);
    assert.ok(Buffer.byteLength(result.stderr) <= 128);
  });
});

test("caller cancellation owns descendants and removes the abort listener", linux, async () => {
  await withFixture({}, async ({ cwd, start }) => {
    const controller = new AbortController();
    // Cancellation must not be suppressible by another consumer of the signal.
    controller.signal.addEventListener("abort", (event) => event.stopImmediatePropagation(), { once: true });
    const work = start({ signal: controller.signal });
    await ready(cwd);
    controller.abort();
    controller.abort();
    const result = await within(work);
    await assertStopped(cwd);
    assert.equal(result.terminationReason, "canceled");
    assert.equal(result.forcedKill, true);
    assert.equal(getEventListeners(controller.signal, "abort").length, 0);
  });
});

test("pre-aborted caller never spawns a process", linux, async () => {
  await withTempDir(async (cwd) => {
    const signal = AbortSignal.abort();
    const result = await runBoundedChild({ command: "no-such-noon-command", cwd, signal });
    assert.equal(result.terminationReason, "canceled");
    assert.equal(result.cleanup.outcome, "not_started");
    assert.equal(getEventListeners(signal, "abort").length, 0);
  });
});

test("spawn failures reject with diagnostics and release cancellation ownership", linux, async () => {
  await withTempDir(async (cwd) => {
    const controller = new AbortController();
    for (const options of [
      { command: "no-such-noon-command", cwd },
      { command: process.execPath, cwd: path.join(cwd, "missing") },
    ]) {
      await assert.rejects(within(runBoundedChild({ ...options, signal: controller.signal })), { code: "ENOENT" });
      assert.equal(getEventListeners(controller.signal, "abort").length, 0);
    }
  });
});

test("normal nonzero exit is not rewritten as cancellation", linux, async () => {
  await withTempDir(async (cwd) => {
    const controller = new AbortController();
    const result = await runBoundedChild({ command: process.execPath, args: ["-e", "process.exit(7)"], cwd, signal: controller.signal });
    controller.abort();
    assert.equal(result.exitCode, 7);
    assert.equal(result.terminationReason, null);
    assert.equal(getEventListeners(controller.signal, "abort").length, 0);
  });
});

test("owner shutdown aborts concurrent groups and subsequent runs recover", linux, async () => {
  const controller = new AbortController();
  let count = 0;
  let release;
  const allReady = new Promise((resolve) => { release = resolve; });
  await Promise.all([0, 1].map(() => withFixture({}, async ({ cwd, start }) => {
    const work = start({ signal: controller.signal });
    await ready(cwd);
    if (++count === 2) { controller.abort(); release(); }
    await within(allReady);
    assert.equal((await within(work)).terminationReason, "canceled");
    await assertStopped(cwd);
  })));
  assert.equal(getEventListeners(controller.signal, "abort").length, 0);
  for (let index = 0; index < 3; index++) {
    await withFixture({ stdio: "ignore", exit: true }, async ({ cwd, start }) => {
      assert.equal((await within(start())).terminationReason, "descendant_cleanup");
      await assertStopped(cwd);
    });
  }
});

test("escaped-session pipes time out explicitly rather than claiming containment", linux, async () => {
  await withFixture({ escaped: true, exit: true }, async ({ cwd, start }) => {
    const result = await within(start());
    assert.equal(result.terminationReason, "cleanup_timeout");
    assert.equal(result.cleanup.outcome, "incomplete");
    assert.equal(result.cleanup.stdioClosed, false);
    assert.equal(await running(await readPid(cwd, "descendant")), true);
    // The escaped child is deliberately cleaned by the harness, NOT the helper.
  });
});

test("signaling failure is a bounded explicit cleanup failure", linux, async (t) => {
  await withFixture({}, async ({ cwd, start }) => {
    const controller = new AbortController();
    const work = start({ signal: controller.signal });
    await ready(cwd);
    const original = process.kill;
    const mock = t.mock.method(process, "kill", (pid, signal) => {
      if (pid < 0) throw Object.assign(new Error("fixture permission denial"), { code: "EPERM" });
      return original(pid, signal);
    });
    try {
      controller.abort();
      const result = await within(work);
      assert.equal(result.cleanup.outcome, "incomplete");
      assert.ok(result.cleanup.errors.some((error) => error.code === "EPERM"));
      assert.equal(result.forcedKill, false);
    } finally { mock.mock.restore(); }
  });
});

test("invalid inputs fail before spawn", async () => {
  assert.throws(() => buildScrubbedEnvironment({ "bad-name": "x" }), /invalid environment key/);
  assert.throws(() => buildScrubbedEnvironment({ BAD: 3 }), /must be a string/);
  assert.throws(() => buildScrubbedEnvironment({ BAD: "\0" }), /NUL/);
  for (const extra of [
    { timeoutMs: 0 }, { killGraceMs: -1 }, { cleanupTimeoutMs: Infinity },
    { maxOutputBytes: 2 ** 32 }, { signal: {} }, { command: "" }, { args: [1] },
  ]) {
    await assert.rejects(runBoundedChild({ command: process.execPath, cwd: os.tmpdir(), ...extra }));
  }
});
