import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const artifactRoot = path.resolve(repoRoot, process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts");
const reportPath = path.join(artifactRoot, "report.json");
const semanticRoot = path.join(artifactRoot, "semantic");
const manifest = JSON.parse(await readFile(path.join(repoRoot, "parity/manim-v0.21/manifest.json"), "utf8"));
const reference = manifest.reference;
const report = JSON.parse(await readFile(reportPath, "utf8"));
// The raster pass already generated this oracle and captured the shared runtime
// at each screenshot. This step only compares artifacts; it never runs a scene.
const manimSemantic = JSON.parse(await readFile(path.join(semanticRoot, "manim-all-frames.json"), "utf8"));
assert.equal(manimSemantic.manim_version, reference.version);
assert.equal(manimSemantic.frame_rate, reference.frame_rate);
const semanticByFixture = new Map(manimSemantic.fixtures.map(fixture => [fixture.id, fixture]));

function maxAbs(values) {
  return values.length === 0 ? 0 : Math.max(...values.map((value) => Math.abs(value)));
}

function vectorDelta(left, right) {
  if (!left || !right) return null;
  return left.map((value, index) => Number(right[index]) - Number(value));
}

function paintDelta(referencePaint, noonPaint) {
  if (referencePaint === null && noonPaint === null) {
    return { presenceMismatch: false, maxChannelDelta: 0 };
  }
  if (referencePaint === null || noonPaint === null) {
    return { presenceMismatch: true, maxChannelDelta: null };
  }
  const fields = ["red", "green", "blue", "alpha"];
  return {
    presenceMismatch: false,
    maxChannelDelta: Math.max(
      ...fields.map((field) => Math.abs(Number(noonPaint[field]) - Number(referencePaint[field]))),
    ),
  };
}

function compareSemanticStates(referenceState, noonState) {
  const noonObjects = noonState.objects.filter((object) => object.present);
  const referenceObjects = referenceState.objects;
  const pairCount = Math.min(referenceObjects.length, noonObjects.length);
  const pairs = [];
  for (let index = 0; index < pairCount; index += 1) {
    const referenceObject = referenceObjects[index];
    const noonObject = noonObjects[index];
    const centerDelta = vectorDelta(referenceObject.center, noonObject.center);
    const boundsComparable = referenceObject.bounds !== null && noonObject.bounds !== null;
    const boundsDelta = boundsComparable
      ? {
          min: vectorDelta(referenceObject.bounds.min, noonObject.bounds.min),
          max: vectorDelta(referenceObject.bounds.max, noonObject.bounds.max),
          width: Number(noonObject.bounds.width) - Number(referenceObject.bounds.width),
          height: Number(noonObject.bounds.height) - Number(referenceObject.bounds.height),
        }
      : null;
    const fill = paintDelta(referenceObject.fill, noonObject.fill);
    const stroke = paintDelta(referenceObject.stroke, noonObject.stroke);
    pairs.push({
      index,
      manimType: referenceObject.type,
      noonObjectId: noonObject.id,
      centerDelta,
      maxCenterDelta: centerDelta === null ? null : maxAbs(centerDelta),
      boundsComparable,
      boundsDelta,
      maxBoundsDelta:
        boundsDelta === null
          ? null
          : maxAbs([
              ...boundsDelta.min,
              ...boundsDelta.max,
              boundsDelta.width,
              boundsDelta.height,
            ]),
      fill,
      stroke,
      strokeWidthDelta: Number(noonObject.stroke_width) - Number(referenceObject.stroke_width),
      noonAppearance: noonObject.appearance,
      noonReveal: noonObject.reveal,
      noonMorph: noonObject.morph,
    });
  }

  const numeric = (field) =>
    pairs.map((pair) => pair[field]).filter((value) => typeof value === "number");
  const paintDeltas = (field) =>
    pairs
      .map((pair) => pair[field].maxChannelDelta)
      .filter((value) => typeof value === "number");
  return {
    pairing: "top-level-render-order",
    referenceObjectCount: referenceObjects.length,
    noonPresentObjectCount: noonObjects.length,
    objectCountDelta: noonObjects.length - referenceObjects.length,
    pairedObjectCount: pairCount,
    maxCenterDelta: maxAbs(numeric("maxCenterDelta")),
    maxBoundsDelta: maxAbs(numeric("maxBoundsDelta")),
    maxFillRgbaDelta: maxAbs(paintDeltas("fill")),
    maxStrokeRgbaDelta: maxAbs(paintDeltas("stroke")),
    maxStrokeWidthDelta: maxAbs(numeric("strokeWidthDelta")),
    paintPresenceMismatches: pairs.filter(
      (pair) => pair.fill.presenceMismatch || pair.stroke.presenceMismatch,
    ).length,
    pairs,
  };
}

const index = [];
for (const fixtureReport of report.fixtures) {
  const fixture = manifest.fixtures.find(entry => entry.id === fixtureReport.id);
  assert.ok(fixture, `${fixtureReport.id}: raster fixture missing from manifest`);
  const manimFixture = semanticByFixture.get(fixture.id);
  assert.ok(manimFixture, `${fixture.id}: missing Manim semantic fixture`);
  assert.equal(manimFixture.frame_count, fixtureReport.manim.frameCount);
  const entries = [];
  for (const [backend, backendReport] of Object.entries(fixtureReport.backends)) {
    assert.ok(!backendReport.error, `${fixture.id}/${backend}: raster execution failed`);
    for (const sample of backendReport.samples) {
      const referenceState = manimFixture.frames[sample.frameIndex];
      const noonState = sample.debugFrame;
      assert.ok(referenceState, `${fixture.id}: missing reference frame ${sample.frameIndex}`);
      assert.ok(noonState?.publication, `${fixture.id}/${backend}: missing shared runtime capture`);
      assert.ok(Math.abs(Number(referenceState.time) - Number(sample.time)) < 1e-9);
      assert.ok(Math.abs(Number(noonState.time) - Number(sample.time)) < 1e-9);
      const comparison = compareSemanticStates(referenceState, noonState);
      const label = `frame-${String(sample.frameIndex).padStart(4, "0")}`;
      const relativePath = path.join("semantic", backend, fixture.id, `${label}.json`);
      const outputPath = path.join(artifactRoot, relativePath);
      await mkdir(path.dirname(outputPath), { recursive: true });
      await writeFile(outputPath, `${JSON.stringify({
        fixture: fixture.id, scene: fixture.scene, backend,
        frameIndex: sample.frameIndex, time: sample.time,
        manim: referenceState, noon: noonState, comparison,
      }, null, 2)}\n`);
      const summary = {
        path: relativePath.split(path.sep).join("/"), pairing: comparison.pairing,
        objectCountDelta: comparison.objectCountDelta,
        maxCenterDelta: comparison.maxCenterDelta, maxBoundsDelta: comparison.maxBoundsDelta,
        maxFillRgbaDelta: comparison.maxFillRgbaDelta, maxStrokeRgbaDelta: comparison.maxStrokeRgbaDelta,
        maxStrokeWidthDelta: comparison.maxStrokeWidthDelta,
        paintPresenceMismatches: comparison.paintPresenceMismatches,
      };
      sample.semantic = summary;
      entries.push({ backend, frameIndex: sample.frameIndex, time: sample.time, ...summary });
    }
  }
  index.push({ id: fixture.id, scene: fixture.scene, samples: entries });
}
report.semantic = {
  schemaVersion: 2, pairing: "top-level-render-order",
  manimAllFrames: "semantic/manim-all-frames.json", index: "semantic/index.json",
  note: "Read-only shared runtime captures from the raster checkpoints. Semantic deltas are diagnostic; existing raster tolerances remain the blocking gate.",
};
await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
await writeFile(path.join(semanticRoot, "index.json"), `${JSON.stringify({
  schemaVersion: 2, manimVersion: reference.version, frameRate: reference.frame_rate, fixtures: index,
}, null, 2)}\n`);
console.log(`Attached captured semantic state to ${index.length} Manim raster fixtures`);
