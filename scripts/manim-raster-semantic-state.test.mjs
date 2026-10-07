import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(repoRoot, "scripts/manim-raster-semantic-state.mjs");

async function runChecker({ frameCount, frames, terminalState, samples, duration = 1,
  frozenIntervals = [], terminalPng }) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-semantic-state-"));
  const artifactRoot = path.join(root, "artifacts");
  const semanticRoot = path.join(artifactRoot, "semantic");
  await mkdir(semanticRoot, { recursive: true });
  const fixture = { id: "sample-fixture", scene: "SampleFixture", expected_duration: duration };
  const manifestPath = path.join(root, "bounded-manifest.json");
  await writeFile(manifestPath, JSON.stringify({
    reference: { version: "0.21.0", frame_rate: 30 },
    fixtures: [fixture],
  }));
  await writeFile(path.join(semanticRoot, "manim-all-frames.json"), JSON.stringify({
    manim_version: "0.21.0",
    frame_rate: 30,
    fixtures: [{
      id: fixture.id,
      frame_count: frameCount,
      frames,
      frozen_intervals: frozenIntervals,
      ...(terminalPng === undefined ? {} : { terminal_png: terminalPng }),
      ...(terminalState === undefined ? {} : { terminal_state: terminalState }),
    }],
  }));
  await writeFile(path.join(artifactRoot, "report.json"), JSON.stringify({
    fixtures: [{
      id: fixture.id,
      scene: fixture.scene,
      manim: { frameCount },
      backends: {
        webgpu: {
          samples: samples.map(sample => ({
            ...sample,
            materializedTime: sample.materializedTime ?? sample.time,
            debugFrame: { time: sample.time, publication: {}, objects: [] },
          })),
        },
      },
    }],
  }));

  const result = spawnSync(process.execPath, [checkerPath], {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      NOON_MANIM_RASTER_ARTIFACTS: artifactRoot,
      NOON_MANIM_RASTER_MANIFEST: manifestPath,
    },
  });
  return { root, artifactRoot, result };
}

test("semantic artifact uses terminal state for a zero-duration terminal-only reference", async () => {
  const terminalState = { time: 0, marker: "zero-duration-terminal", objects: [] };
  const run = await runChecker({
    frameCount: 0,
    frames: [],
    duration: 0,
    terminalState,
    samples: [{ frameIndex: null, referenceKind: "terminal", terminalState: true, time: 0 }],
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const artifact = JSON.parse(await readFile(
      path.join(run.artifactRoot, "semantic/webgpu/sample-fixture/terminal.json"), "utf8",
    ));
    assert.equal(artifact.manim.marker, "zero-duration-terminal");
    assert.equal(artifact.referenceKind, "terminal");
    const report = JSON.parse(await readFile(path.join(run.artifactRoot, "report.json"), "utf8"));
    assert.equal(report.fixtures[0].backends.webgpu.samples[0].semantic.referenceKind, "terminal");
    assert.match(report.fixtures[0].backends.webgpu.samples[0].semantic.path, /terminal\.json$/);
  } finally {
    await rm(run.root, { recursive: true, force: true });
  }
});

test("semantic artifact uses terminal state at a positive-duration endpoint", async () => {
  const run = await runChecker({
    frameCount: 2,
    frames: [{ time: 0, marker: "first" }, { time: 1, marker: "last-png" }],
    terminalState: { time: 1, marker: "authored-terminal", objects: [] },
    samples: [{ frameIndex: null, referenceKind: "terminal", terminalState: true, time: 1 }],
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const artifact = JSON.parse(await readFile(
      path.join(run.artifactRoot, "semantic/webgpu/sample-fixture/terminal.json"), "utf8",
    ));
    assert.equal(artifact.manim.marker, "authored-terminal");
  } finally {
    await rm(run.root, { recursive: true, force: true });
  }
});

test("sequence sample resolves its indexed Manim frame and keeps a sequence label", async () => {
  const run = await runChecker({
    frameCount: 2,
    frames: [{ time: 0, marker: "first", objects: [] }, { time: 1, marker: "indexed-frame", objects: [] }],
    terminalState: { time: 1, marker: "terminal" },
    samples: [{ frameIndex: 1, referenceKind: "sequence", terminalState: false, time: 1 }],
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const artifact = JSON.parse(await readFile(
      path.join(run.artifactRoot, "semantic/webgpu/sample-fixture/frame-0001.json"), "utf8",
    ));
    assert.equal(artifact.manim.marker, "indexed-frame");
    assert.equal(artifact.referenceKind, "sequence");
  } finally {
    await rm(run.root, { recursive: true, force: true });
  }
});

test("a held sequence endpoint keeps an artifact distinct from its materialized frame", async () => {
  const run = await runChecker({
    frameCount: 1,
    frames: [{ time: 1, objects: [] }],
    terminalState: { time: 2, objects: [] },
    samples: [
      { frameIndex: 0, referenceKind: "sequence", terminalState: false, time: 1, materializedTime: 1 },
      { frameIndex: 0, referenceKind: "sequence", terminalState: true, time: 2, materializedTime: 1 },
    ],
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const report = JSON.parse(await readFile(path.join(run.artifactRoot, "report.json"), "utf8"));
    const samples = report.fixtures[0].backends.webgpu.samples;
    assert.notEqual(samples[0].semantic.path, samples[1].semantic.path);
    for (const sample of samples) {
      const artifact = JSON.parse(await readFile(path.join(run.artifactRoot, sample.semantic.path), "utf8"));
      assert.equal(artifact.time, sample.time);
      assert.equal(artifact.manim.time, 1);
      assert.equal(artifact.noon.time, sample.time);
    }
  } finally {
    await rm(run.root, { recursive: true, force: true });
  }
});

test("sequence, repeated frozen holds, and terminal retain distinct logical and materialized times", async () => {
  const run = await runChecker({
    frameCount: 3,
    duration: 3,
    frames: [0, 1, 2].map(time => ({ time, marker: `frame-${time}`, objects: [] })),
    frozenIntervals: [{ frame_index: 2, start_time: 2, end_time: 3 }],
    terminalPng: { path: "terminal.png" },
    terminalState: { time: 3, marker: "terminal", objects: [] },
    samples: [
      { frameIndex: 2, referenceKind: "sequence", time: 2, materializedTime: 2 },
      { frameIndex: 2, referenceKind: "frozen-hold", time: 2.5, materializedTime: 2 },
      { frameIndex: 2, referenceKind: "frozen-hold", time: 2.75, materializedTime: 2 },
      { frameIndex: null, referenceKind: "terminal", terminalState: true, time: 3 },
    ],
  });
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const dir = path.join(run.artifactRoot, "semantic/webgpu/sample-fixture");
    for (const name of ["frame-0002.json", "frame-0002-hold-2_5.json",
      "frame-0002-hold-2_75.json", "terminal.json"]) {
      await readFile(path.join(dir, name), "utf8");
    }
    const hold = JSON.parse(await readFile(path.join(dir, "frame-0002-hold-2_5.json"), "utf8"));
    assert.equal(hold.manim.time, 2);
    assert.equal(hold.noon.time, 2.5);
    const hold2 = JSON.parse(await readFile(path.join(dir, "frame-0002-hold-2_75.json"), "utf8"));
    assert.equal(hold2.manim.time, 2);
    assert.equal(hold2.noon.time, 2.75);
  } finally {
    await rm(run.root, { recursive: true, force: true });
  }
});

test("frozen hold outside the recorded interval fails closed", async (t) => {
  for (const item of [
    { name: "no interval", frozenIntervals: [] },
    { name: "interval ends before sample", frozenIntervals: [
      { frame_index: 2, start_time: 2, end_time: 2.4 },
    ] },
  ]) {
    await t.test(item.name, async () => {
      const run = await runChecker({
        frameCount: 3,
        duration: 3,
        frames: [0, 1, 2].map(time => ({ time, objects: [] })),
        frozenIntervals: item.frozenIntervals,
        terminalState: { time: 3, objects: [] },
        samples: [{ frameIndex: 2, referenceKind: "frozen-hold", time: 2.5, materializedTime: 2 }],
      });
      try {
        assert.notEqual(run.result.status, 0);
        assert.match(run.result.stderr, /no reference frame at requested logical time 2\.5/);
      } finally {
        await rm(run.root, { recursive: true, force: true });
      }
    });
  }
});

test("malformed samples fail closed for missing terminal state or invalid frame indices", async (t) => {
  const cases = [
    {
      name: "missing terminal state",
      frameCount: 0,
      frames: [],
      samples: [{ frameIndex: null, referenceKind: "terminal", terminalState: true, time: 0 }],
      error: /missing terminal Manim state/,
    },
    {
      name: "missing sequence frame index",
      frameCount: 1,
      frames: [{ time: 0, objects: [] }],
      terminalState: { time: 1, objects: [] },
      samples: [{ referenceKind: "sequence", time: 0 }],
      error: /invalid sequence frame index undefined/,
    },
    {
      name: "non-integer sequence frame index",
      frameCount: 1,
      frames: [{ time: 0, objects: [] }],
      terminalState: { time: 1, objects: [] },
      samples: [{ frameIndex: 0.5, referenceKind: "sequence", time: 0 }],
      error: /invalid sequence frame index 0\.5/,
    },
  ];
  for (const item of cases) {
    await t.test(item.name, async () => {
      const run = await runChecker(item);
      try {
        assert.notEqual(run.result.status, 0);
        assert.match(run.result.stderr, item.error);
      } finally {
        await rm(run.root, { recursive: true, force: true });
      }
    });
  }
});
