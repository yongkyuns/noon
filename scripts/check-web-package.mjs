import { readFile, stat } from "node:fs/promises";
import { join } from "node:path";

const packageDirectory = join(process.cwd(), "web", "pkg");
const javascriptPath = join(packageDirectory, "noon_web.js");
const declarationsPath = join(packageDirectory, "noon_web.d.ts");
const wasmPath = join(packageDirectory, "noon_web_bg.wasm");

const [javascript, declarations, wasm, wasmStats] = await Promise.all([
  readFile(javascriptPath, "utf8"),
  readFile(declarationsPath, "utf8"),
  readFile(wasmPath),
  stat(wasmPath),
]);

const expectedJavascriptSurface = [
  "export class NoonCanvasPlayer",
  "static create(",
  "applyPatchBatch(",
  "replaceScene(",
  "reconcileScene(",
  "renderFrame(",
  "nextSequence(",
  "gpuProfilingSupported(",
  "setGpuProfilingEnabled(",
  "gpuRenderP50Ms(",
  "gpuRenderP95Ms(",
  "lastCpuFrameMs(",
  "lastRuntimeEvaluationMs(",
  "lastFramePrepareMs(",
  "lastUploadMs(",
  "lastEncodeSubmitMs(",
  "lastGeometryCacheMisses(",
  "export class HostScenePlayer",
  "advanceTo(",
  "callbackFrameJson(",
  "commitPatchBatch(",
  "export class AuthoringSceneCore",
  "export class DetachedMobjectCore",
  "export class AnimateCore",
  "export class PlayBatchCore",
  "export function authoringCircle(",
  "export function authoringSquare(",
  "export function authoringRectangle(",
  "export function authoringLine(",
  "createPlayBatch(",
  "appendAnimate(",
  "appendCreate(",
  "appendFadeOut(",
  "appendFadeIn(",
  "appendTransform(",
  "playBatch(",
  "sceneJson(",
  "export function evaluateSceneSnapshot(",
  "export function evaluateScenePlaybackSnapshot(",
  "export function demoSceneJson(",
  "export function resolveAnimationOptions(",
  "export function resolveCompositionSchedule(",
  "export function resolveUniformCompositionSchedule(",
  "export function resolveLifecyclePlan(",
  "export function validatePresenceTransition(",
];
const expectedTypeSurface = [
  "export class NoonCanvasPlayer",
  "static create(",
  "applyPatchBatch(json: string): void",
  "replaceScene(json: string): void",
  "reconcileScene(json: string): boolean",
  "renderFrame(timestamp_ms: number): boolean",
  "nextSequence(): bigint",
  "gpuProfilingSupported(): boolean",
  "setGpuProfilingEnabled(enabled: boolean): boolean",
  "gpuRenderP50Ms(): number",
  "gpuRenderP95Ms(): number",
  "lastCpuFrameMs(): number",
  "lastRuntimeEvaluationMs(): number",
  "lastFramePrepareMs(): number",
  "lastUploadMs(): number",
  "lastEncodeSubmitMs(): number",
  "lastGeometryCacheMisses(): number",
  "export class HostScenePlayer",
  "constructor(scene_json: string, callback_slots_json: string)",
  "advanceTo(time: number): void",
  "callbackFrameJson(): string",
  "commitPatchBatch(json: string): void",
  "export class AuthoringSceneCore",
  "constructor()",
  "add(object: DetachedMobjectCore): number",
  "animate(handle: number): AnimateCore",
  "createPlayBatch(): PlayBatchCore",
  "appendAnimate(batch: PlayBatchCore, animation: AnimateCore): void",
  "appendCreate(batch: PlayBatchCore, handle: number): void",
  "appendFadeOut(batch: PlayBatchCore, handle: number): void",
  "appendFadeIn(batch: PlayBatchCore, handle: number): void",
  "appendTransform(batch: PlayBatchCore, handle: number, target: DetachedMobjectCore): void",
  "playBatch(batch: PlayBatchCore, run_time: number, rate_func: string): void",
  "sceneJson(): string",
  "export class DetachedMobjectCore",
  "export class AnimateCore",
  "export class PlayBatchCore",
  "export function authoringCircle(radius: number): DetachedMobjectCore",
  "export function authoringSquare(side_length: number): DetachedMobjectCore",
  "export function authoringRectangle(width: number, height: number): DetachedMobjectCore",
  "export function authoringLine(start_x: number, start_y: number, end_x: number, end_y: number): DetachedMobjectCore",
  "export function evaluateSceneSnapshot(scene_json: string, time: number): string",
  "export function evaluateScenePlaybackSnapshot(scene_json: string, times_json: string): string",
  "export function demoSceneJson(): string",
  "export function resolveAnimationOptions(",
  "export function resolveCompositionSchedule(",
  "export function resolveUniformCompositionSchedule(",
  "export function resolveLifecyclePlan(",
  "export function validatePresenceTransition(",
];

for (const fragment of expectedJavascriptSurface) {
  if (!javascript.includes(fragment)) {
    throw new Error(`Generated JavaScript is missing: ${fragment}`);
  }
}
for (const fragment of expectedTypeSurface) {
  if (!declarations.includes(fragment)) {
    throw new Error(`Generated declarations are missing: ${fragment}`);
  }
}
if (wasmStats.size === 0) {
  throw new Error("Generated WebAssembly module is empty");
}

await WebAssembly.compile(wasm);
console.log(
  `Validated browser package: ${wasmStats.size} byte wasm module and ${expectedJavascriptSurface.length} public API contracts.`,
);
