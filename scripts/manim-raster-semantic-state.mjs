import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const manifestPath = path.join(repoRoot, "parity", "manim-v0.21", "manifest.json");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const reference = manifest.reference;
const fixtureSource = await readFile(path.join(repoRoot, reference.source), "utf8");
const artifactRoot = path.resolve(
  repoRoot,
  process.env.NOON_MANIM_RASTER_ARTIFACTS ?? "manim-raster-artifacts",
);
const reportPath = path.join(artifactRoot, "report.json");
const semanticRoot = path.join(artifactRoot, "semantic");
const manimSemanticPath = path.join(semanticRoot, "manim-all-frames.json");
const semanticIndexPath = path.join(semanticRoot, "index.json");
const port = Number(process.env.NOON_MANIM_SEMANTIC_PORT ?? "4193");
const baseUrl = `http://127.0.0.1:${port}`;

function runChecked(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed (${result.status})\n${result.stdout}\n${result.stderr}`,
    );
  }
  return result;
}

function noonSourceFor(scene) {
  const adapted = fixtureSource.replace("from manim import *", "from noon import *");
  return `${adapted}\n\nresult = ${scene}()\nresult.setup()\ntry:\n    result.construct()\nfinally:\n    result.tear_down()\n`;
}

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

let serverOutput = "";
const server = spawn(
  "python3",
  ["-m", "http.server", String(port), "--bind", "127.0.0.1", "--directory", repoRoot],
  { cwd: repoRoot, stdio: ["ignore", "pipe", "pipe"] },
);
server.stdout.on("data", (chunk) => (serverOutput += chunk));
server.stderr.on("data", (chunk) => (serverOutput += chunk));

async function waitForServer() {
  let lastError = null;
  for (let attempt = 0; attempt < 80; attempt += 1) {
    try {
      const response = await fetch(`${baseUrl}/web/manim-compat-smoke.html`);
      if (response.ok) return;
      lastError = new Error(`HTTP ${response.status}`);
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`semantic-state server did not start: ${lastError}\n${serverOutput}`);
}

await mkdir(semanticRoot, { recursive: true });
runChecked("python3", [
  path.join("scripts", "manim-raster-semantic-reference.py"),
  "--manifest",
  manifestPath,
  "--output",
  manimSemanticPath,
]);

try {
  await waitForServer();
  const report = JSON.parse(await readFile(reportPath, "utf8"));
  const manimSemantic = JSON.parse(await readFile(manimSemanticPath, "utf8"));
  assert.equal(manimSemantic.manim_version, reference.version, "semantic Manim version");
  assert.equal(manimSemantic.frame_rate, reference.frame_rate, "semantic Manim frame rate");
  const semanticByFixture = new Map(
    manimSemantic.fixtures.map((fixture) => [fixture.id, fixture]),
  );

  const browser = await chromium.launch({
    channel: "chromium",
    headless: true,
    args: ["--disable-dev-shm-usage"],
  });
  const index = [];
  try {
    const page = await browser.newPage();
    await page.goto(`${baseUrl}/web/manim-compat-smoke.html`, { waitUntil: "load" });
    await page.waitForFunction(() => window.noonManimCompat, null, { timeout: 30_000 });
    await page.evaluate(() => window.noonManimCompat.ready());

    for (const fixtureReport of report.fixtures) {
      const fixture = manifest.fixtures.find((entry) => entry.id === fixtureReport.id);
      assert.ok(fixture, `${fixtureReport.id}: raster fixture missing from manifest`);
      const authored = await page.evaluate(
        (source) => window.noonManimCompat.run(source),
        noonSourceFor(fixture.scene),
      );
      assert.equal(authored.kind, "scene_document", `${fixture.id}: Noon semantic authoring result`);
      assert.equal(authored.duration, fixture.expected_duration, `${fixture.id}: Noon semantic duration`);
      const sceneJson = JSON.stringify(authored.document);
      const manimFixture = semanticByFixture.get(fixture.id);
      assert.ok(manimFixture, `${fixture.id}: missing Manim semantic fixture`);
      assert.equal(
        manimFixture.frame_count,
        fixtureReport.manim.frameCount,
        `${fixture.id}: semantic/raster Manim frame count`,
      );

      const firstBackend = Object.values(fixtureReport.backends)[0];
      const entries = [];
      for (const sample of firstBackend.samples) {
        const referenceState = manimFixture.frames[sample.frameIndex];
        assert.ok(referenceState, `${fixture.id}: missing semantic frame ${sample.frameIndex}`);
        assert.ok(
          Math.abs(Number(referenceState.time) - Number(sample.time)) < 1e-9,
          `${fixture.id}: semantic/raster time mismatch at frame ${sample.frameIndex}`,
        );
        const noonState = await page.evaluate(
          ({ json, time }) => window.noonManimCompat.semanticFrame(json, time),
          { json: sceneJson, time: sample.time },
        );
        const comparison = compareSemanticStates(referenceState, noonState);
        const label = `frame-${String(sample.frameIndex).padStart(4, "0")}`;
        const relativePath = path.join("semantic", fixture.id, `${label}.json`);
        const outputPath = path.join(artifactRoot, relativePath);
        await mkdir(path.dirname(outputPath), { recursive: true });
        await writeFile(
          outputPath,
          `${JSON.stringify(
            {
              fixture: fixture.id,
              scene: fixture.scene,
              frameIndex: sample.frameIndex,
              time: sample.time,
              manim: referenceState,
              noon: noonState,
              comparison,
            },
            null,
            2,
          )}\n`,
        );
        const summary = {
          path: relativePath.split(path.sep).join("/"),
          pairing: comparison.pairing,
          objectCountDelta: comparison.objectCountDelta,
          maxCenterDelta: comparison.maxCenterDelta,
          maxBoundsDelta: comparison.maxBoundsDelta,
          maxFillRgbaDelta: comparison.maxFillRgbaDelta,
          maxStrokeRgbaDelta: comparison.maxStrokeRgbaDelta,
          maxStrokeWidthDelta: comparison.maxStrokeWidthDelta,
          paintPresenceMismatches: comparison.paintPresenceMismatches,
        };
        for (const backendReport of Object.values(fixtureReport.backends)) {
          const backendSample = backendReport.samples.find(
            (entry) => entry.frameIndex === sample.frameIndex,
          );
          assert.ok(backendSample, `${fixture.id}: backend missing frame ${sample.frameIndex}`);
          backendSample.semantic = summary;
        }
        entries.push({ frameIndex: sample.frameIndex, time: sample.time, ...summary });
      }
      index.push({ id: fixture.id, scene: fixture.scene, samples: entries });
    }
  } finally {
    await browser.close();
  }

  report.semantic = {
    schemaVersion: 1,
    pairing: "top-level-render-order",
    manimAllFrames: "semantic/manim-all-frames.json",
    index: "semantic/index.json",
    note:
      "Semantic deltas are diagnostic. Existing Manim semantic differential tests and raster tolerances remain the blocking compatibility gates.",
  };
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  await writeFile(
    semanticIndexPath,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        manimVersion: reference.version,
        frameRate: reference.frame_rate,
        fixtures: index,
      },
      null,
      2,
    )}\n`,
  );
  console.log(`Attached semantic state to ${index.length} Manim raster fixtures`);
} finally {
  server.kill("SIGTERM");
}
