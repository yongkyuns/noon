// Stage exact ManimCE example classes for the existing raster differential.
// This creates no qualification pass; unsupported Noon operations stay failures.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const PINNED_DOCS_COMMIT = "861cd4849b17db1db3515b531ffe80b297848f93";
export const PINNED_DOCS_URL = "https://raw.githubusercontent.com/ManimCommunity/manim/" +
  PINNED_DOCS_COMMIT + "/docs/source/examples.rst";
export const PINNED_DOCS_SHA256 = "0605e53c8c44dda386fa92e923a85fcad8150a0c8690eaa24138f84d081b3e0c";
export const CAMERA_SOURCE_HASHES = Object.freeze({
  MovingZoomedSceneAround: "08416a524ae4bfb7e53e67ccec8c8479dc23ef81cd52d10806b24f6f8abb0943",
  FixedInFrameMObjectTest: "f29be595f767cb19cc350b5eb3a0b7c935724a3bc0c4d0bcd4795224a275987f",
  ThreeDLightSourcePosition: "d7323956511ec9c71490fd487e8261728081b4d3fdd5473eb452751f6e0989a7",
  ThreeDCameraRotation: "3183a2612392faad7e92bc4f30c091aa3db0faea6280d498f5bfc32c87bb1df2",
  ThreeDCameraIllusionRotation: "e2b822cbf76f97ab8d8fa2a81d782f66b07b1a959a67369fbfcf0336895c7f0e",
  ThreeDSurfacePlot: "ace9505560e9ae3e666fcb24c73591f4094510578477f8a8f28c72d3a2025b3e",
});

// Observe numeric 3D camera state as well as pixels, including both sides of
// the ambient/finite-motion barriers. Matching settled images alone cannot
// qualify the transition or its exact authored endpoint.
export const PINNED_CAMERA_CASES = Object.freeze([
  ["MovingZoomedSceneAround", 12, [0.5, 1.5, 3.5, 9.5, 12], false],
  ["FixedInFrameMObjectTest", 1, [0, 0.5, 1], true],
  ["ThreeDLightSourcePosition", 0, [0], true],
  ["ThreeDCameraRotation", 3,
    [0, 0.5, 29 / 30, 1, 31 / 30, 1.5, 59 / 30, 2, 61 / 30, 2.5, 3], true],
  ["ThreeDCameraIllusionRotation", Math.PI / 2, [0, 0.5, 1, 47 / 30, Math.PI / 2], true],
  ["ThreeDSurfacePlot", 0, [0], true],
]);
const digest = value => createHash("sha256").update(value).digest("hex");
const CLASS_AST = "import ast,sys; m=ast.parse(sys.stdin.read()); " +
  "assert len(m.body)==1 and isinstance(m.body[0],ast.ClassDef) and m.body[0].name==sys.argv[1]";

export function extractPinnedScene(rst, sceneName, expectedSha256 = CAMERA_SOURCE_HASHES[sceneName]) {
  const lines = rst.split(/\r?\n/);
  const directive = lines.findIndex(line => line.trim() === ".. manim:: " + sceneName);
  assert.notEqual(directive, -1, "pinned docs missing " + sceneName);
  const next = lines.findIndex((line, index) => index > directive && /^\.\. manim:: /.test(line));
  const block = lines.slice(directive + 1, next < 0 ? undefined : next);
  const classIndex = block.findIndex(line => new RegExp("^\\s+class " + sceneName + "\\(").test(line));
  assert.notEqual(classIndex, -1, "pinned directive has no " + sceneName + " class");
  const indent = block[classIndex].length - block[classIndex].trimStart().length;
  const code = [];
  for (const line of block.slice(classIndex)) {
    const lineIndent = line.length - line.trimStart().length;
    if (line.trim() && lineIndent < indent) break;
    code.push(line.trim() ? line.slice(indent) : "");
  }
  const source = code.join("\n").trimEnd() + "\n";
  assert.equal(digest(source), expectedSha256, sceneName + ": pinned class source drift");
  const parsed = spawnSync("python3", ["-c", CLASS_AST, sceneName], { input: source, encoding: "utf8" });
  assert.equal(parsed.status, 0, sceneName + ": extracted source must parse as one class: " + parsed.stderr);
  return source;
}

function assertModuleClassMatches(source, module, sceneName) {
  const checker = [
    "import ast,json,sys",
    "scene,expected=json.load(sys.stdin)",
    "expected_class=ast.parse(expected).body[0]",
    "actual_classes=[n for n in ast.parse(sys.argv[1]).body if isinstance(n,ast.ClassDef)]",
    "assert len(actual_classes)==1 and actual_classes[0].name==scene",
    "assert ast.dump(actual_classes[0],include_attributes=False)==ast.dump(expected_class,include_attributes=False)",
  ].join(";");
  const result = spawnSync("python3", ["-c", checker, module], {
    input: JSON.stringify([sceneName, source]), encoding: "utf8",
  });
  assert.equal(result.status, 0, sceneName + ": generated module changed pinned class AST: " + result.stderr);
}

export async function stageSpecialCameraRasterManifest({ rst, repositoryRoot, outputDirectory }) {
  assert.equal(digest(rst), PINNED_DOCS_SHA256, "pinned Manim examples.rst digest drift");
  const stageRoot = path.resolve(outputDirectory);
  const temporaryRoot = path.resolve(tmpdir());
  assert.ok(stageRoot.startsWith(temporaryRoot + path.sep),
    "generated camera evidence must be staged beneath the system temporary directory");
  await mkdir(stageRoot, { recursive: true });
  const baseline = JSON.parse(await readFile(
    path.join(repositoryRoot, "parity/manim-v0.21/manifest.json"), "utf8"));
  const fixtures = [];
  const sources = [];
  for (const [scene, duration, sampleTimes, cameraProfileObservation] of PINNED_CAMERA_CASES) {
    const pinnedClass = extractPinnedScene(rst, scene);
    // The docs gallery supplies these globals around class-only RST snippets.
    const module = "from manim import *\nimport numpy as np\n\n" + pinnedClass;
    assertModuleClassMatches(pinnedClass, module, scene);
    const file = scene + ".py";
    await writeFile(path.join(stageRoot, file), module);
    sources.push({ scene, file, classSha256: digest(pinnedClass), moduleSha256: digest(module) });
    fixtures.push({
      id: "pinned-camera-" + scene,
      scene,
      source: path.relative(repositoryRoot, path.join(stageRoot, file)).split(path.sep).join("/"),
      expected_duration: duration,
      sample_times: sampleTimes,
      camera_profile_observation: cameraProfileObservation,
      notes: "Exact pinned Manim source staged for observation. Unsupported Noon capabilities remain failures; no source compensation or parity claim.",
    });
  }
  const staged = { ...baseline, reference: { ...baseline.reference, source: fixtures[0].source }, fixtures };
  const manifestPath = path.join(stageRoot, "manifest.json");
  await writeFile(manifestPath, JSON.stringify(staged, null, 2) + "\n");
  await writeFile(path.join(stageRoot, "source-provenance.json"), JSON.stringify({
    upstream: PINNED_DOCS_URL,
    commit: PINNED_DOCS_COMMIT,
    docsSha256: PINNED_DOCS_SHA256,
    fixtures: sources,
    status: "staged-only; no raster qualification claimed",
  }, null, 2) + "\n");
  return { manifestPath, stageRoot, fixtures: fixtures.map(fixture => fixture.id), sources };
}

async function main() {
  const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const response = await fetch(PINNED_DOCS_URL);
  assert.equal(response.status, 200, "pinned docs download failed: " + response.status);
  const staged = await stageSpecialCameraRasterManifest({
    rst: await response.text(),
    repositoryRoot,
    outputDirectory: path.join(tmpdir(), "noon-phase-d-finish", "exact-camera-stage"),
  });
  console.log("Staged " + staged.fixtures.length + " exact-source fixtures: " + staged.manifestPath);
  console.log("Next: NOON_MANIM_RASTER_MANIFEST=" + staged.manifestPath + " " +
    "NOON_MANIM_RASTER_ENFORCE=0 node scripts/manim-raster-differential.mjs");
  console.log("Unsupported Noon capabilities may fail; this stage is not a parity pass.");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
