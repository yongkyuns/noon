import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { WorkerCpuProfileCapture } from "./worker-cpu-profile.mjs";

class FakeBrowserCdp extends EventEmitter {
  targets = [
    { targetId: "python", type: "worker", url: "http://localhost/web/python-worker.js" },
    { targetId: "render", type: "worker", url: "http://localhost/web/execution-render-worker.js" },
  ];

  async send(method, params) {
    if (method === "Target.getTargets") return { targetInfos: this.targets };
    if (method === "Target.attachToTarget") return { sessionId: `session-${params.targetId}` };
    if (method === "Target.sendMessageToTarget") {
      const command = JSON.parse(params.message);
      const profile = { startTime: 10, endTime: 11, nodes: [{ id: 1, callFrame: { functionName: "tick" } }],
        samples: [1], timeDeltas: [1_000] };
      setImmediate(() => this.emit("Target.receivedMessageFromTarget", {
        sessionId: params.sessionId,
        message: JSON.stringify({ id: command.id, result: command.method === "Profiler.stop" ? { profile } : {} }),
      }));
      return {};
    }
    if (method === "Target.setDiscoverTargets") return {};
    throw new Error(`unexpected CDP command ${method}`);
  }
}

test("worker CPU capture routes profiles through attached dedicated-worker targets", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "noon-cpu-profile-"));
  try {
    const capture = new WorkerCpuProfileCapture(new FakeBrowserCdp());
    await capture.attach();
    await assert.rejects(capture.attach(), /cannot attach profiler in attached state/);
    const boundary = { runGeneration: 4, session: 9, time: 8.7, rendererAt: 4_000,
      exampleId: "showcase-bezier-paths" };
    await capture.start(boundary);
    await assert.rejects(capture.start(boundary), /cannot start profiler in started state/);
    await assert.rejects(capture.stop({ ...boundary, time: 9.5, rendererAt: 4_100 }, directory),
      /new completed run/);
    const end = { runGeneration: 5, session: 10, time: 9.5, rendererAt: 4_100,
      exampleId: "showcase-bezier-paths" };
    const result = await capture.stop(end, directory);
    await assert.rejects(capture.stop(end, directory),
      /cannot stop profiler in stopped state/);
    assert.equal(result.status, "captured");
    assert.deepEqual(Object.keys(result.profiles).sort(), ["authoring", "render"]);
    for (const owner of ["authoring", "render"]) {
      const profile = result.profiles[owner];
      assert.equal(profile.sampleCount, 1);
      assert.equal(JSON.parse(await readFile(path.join(directory, profile.file), "utf8")).samples[0], 1);
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("worker CPU capture cannot stop before start", async () => {
  const capture = new WorkerCpuProfileCapture(new FakeBrowserCdp());
  await capture.attach();
  await assert.rejects(capture.stop({ runGeneration: 4, session: 9, time: 3.6, rendererAt: 2_350 }, os.tmpdir()),
    /cannot stop profiler in attached state/);
});

test("worker CPU capture fails closed when a required worker target is absent", async () => {
  const cdp = new FakeBrowserCdp();
  cdp.targets = cdp.targets.filter(target => target.targetId !== "render");
  await assert.rejects(new WorkerCpuProfileCapture(cdp).attach(), /expected one live render worker target/);
});
