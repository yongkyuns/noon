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
  "renderFrame(",
  "nextSequence(",
  "export function demoSceneJson(",
];
const expectedTypeSurface = [
  "export class NoonCanvasPlayer",
  "static create(",
  "applyPatchBatch(json: string): void",
  "renderFrame(timestamp_ms: number): boolean",
  "nextSequence(): bigint",
  "export function demoSceneJson(): string",
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
