import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, mkdir, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const checkerPath = path.join(repoRoot, "scripts/manim-raster-semantic-state.mjs");

async function runChecker({ frameCount, frames, terminalState, samples }) {
  const root = await mkdtemp(path.join(os.tmpdir(), "noon-semantic-state-"));
  const artifactRoot = path.join(root, "artifacts");
  const semanticRoot = path.join(artifactRoot, "semantic");
  await mkdir(semanticRoot, { recursive: true });
  const fixture = { id: "sample-fixture", scene: "SampleFixture" };
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
