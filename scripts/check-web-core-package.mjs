import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";

const packageDirectory = join(process.cwd(), "web", "pkg-core");
const javascriptPath = join(packageDirectory, "noon_web.js");
const declarationsPath = join(packageDirectory, "noon_web.d.ts");
const wasmPath = join(packageDirectory, "noon_web_bg.wasm");

const [javascript, declarations, wasm, wasmStats] = await Promise.all([
  readFile(javascriptPath, "utf8"),
  readFile(declarationsPath, "utf8"),
  readFile(wasmPath),
  stat(wasmPath),
]);

const required = [
  "export class WasmAuthoringStore",
  "export class RetainedNativeTextAuthoringHandle",
  "export class RetainedTypstAuthoringHandle",
  "export class SemanticExecutionPlayer",
  "export class EngineScenePlayer",
  "export class CanonicalRetainedEngineScenePlayer",
  "export function canonicalRetainedSceneSpecJson(",
  "export function manimAnnularSectorSnapshotJson(",
  "export function manimAnnulusSnapshotJson(",
  "export function manimDashedLineSnapshotJson(",
  "export function manimDotSnapshotJson(",
  "export function manimElbowSnapshotJson(",
  "export function manimRoundedRectangleSnapshotJson(",
  "export function manimSectorSnapshotJson(",
  "export function manimTriangleSnapshotJson(",
  "export function manimUnderlineSnapshotJson(",
  "export function resolveAnimationOptions(",
  "export function resolveCompositionSchedule(",
  "export function resolveLifecyclePlan(",
  "export function resolveUniformCompositionSchedule(",
  "export function validatePresenceTransition(",
];

const rendererOnly = [
  "export class ExecutionCanvasRenderer",
  "export class RetainedExecutionCanvasRenderer",
];

for (const fragment of required) {
  if (!javascript.includes(fragment)) {
    throw new Error(`Generated core JavaScript is missing: ${fragment}`);
  }
  if (!declarations.includes(fragment)) {
    throw new Error(`Generated core declarations are missing: ${fragment}`);
  }
}
for (const fragment of rendererOnly) {
  if (javascript.includes(fragment) || declarations.includes(fragment)) {
    throw new Error(`Generated core package must not expose renderer API: ${fragment}`);
  }
}
if (wasmStats.size === 0) {
  throw new Error("Generated core WebAssembly module is empty");
}

await WebAssembly.compile(wasm);
console.log(
  `Validated browser core package: ${wasmStats.size} byte wasm module, ${required.length} required core contracts, renderer exports absent.`,
);
