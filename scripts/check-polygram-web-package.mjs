import { readFile } from "node:fs/promises";
import path from "node:path";

const packageDirectory = path.join(process.cwd(), "web", "pkg");
const [javascript, declarations] = await Promise.all([
  readFile(path.join(packageDirectory, "noon_web.js"), "utf8"),
  readFile(path.join(packageDirectory, "noon_web.d.ts"), "utf8"),
]);

const javascriptSurface = [
  "export class WasmPolygramVertexGroups",
  "createManimPolygon(",
  "createManimPolygram(",
  "createManimRegularPolygon(",
  "createManimRegularPolygram(",
  "createManimTriangle(",
  "createManimStar(",
  "manimVertexGroups(",
  "coordinates(",
  "groupLengths(",
];

const declarationSurface = [
  "export class WasmPolygramVertexGroups",
  "createManimPolygon(",
  "createManimPolygram(",
  "createManimRegularPolygon(",
  "createManimRegularPolygram(",
  "createManimTriangle(",
  "createManimStar(",
  "manimVertexGroups(): WasmPolygramVertexGroups",
  "coordinates(): Float64Array",
  "groupLengths(): Uint32Array",
];

for (const fragment of javascriptSurface) {
  if (!javascript.includes(fragment)) {
    throw new Error(`Generated JavaScript is missing polygon adapter surface: ${fragment}`);
  }
}
for (const fragment of declarationSurface) {
  if (!declarations.includes(fragment)) {
    throw new Error(`Generated declarations are missing polygon adapter surface: ${fragment}`);
  }
}

console.log("Validated shared polygon/polygram browser package surface");
