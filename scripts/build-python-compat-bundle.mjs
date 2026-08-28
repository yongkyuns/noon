import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const webRoot = path.join(repoRoot, "web");
const outputPath = path.join(webRoot, "python", "compat-bundle.json");

const modules = await Promise.all(
  PYTHON_COMPAT_MODULES.map(async ({ sourcePath, runtimePath, label }) => ({
    runtimePath,
    label,
    source: await readFile(path.join(webRoot, sourcePath), "utf8"),
  })),
);

await writeFile(
  outputPath,
  `${JSON.stringify({ version: 1, modules })}\n`,
  "utf8",
);

console.log(`✓ bundled ${modules.length} Python compatibility modules`);
