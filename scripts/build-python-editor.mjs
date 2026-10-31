import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../tools/python-editor/", import.meta.url));
const output = fileURLToPath(new URL("../web/pkg/", import.meta.url));
const require = createRequire(new URL("../tools/python-editor/package.json", import.meta.url));
const { build } = require("esbuild");
const result = await build({
  absWorkingDir: root,
  entryPoints: ["dependencies.js"],
  outfile: path.join(output, "python-editor-bundle.js"),
  bundle: true,
  format: "esm",
  platform: "browser",
  target: "es2022",
  minify: true,
  metafile: true,
  legalComments: "none",
  banner: { js: "/*! CodeMirror dependency licenses: ./python-editor-licenses.txt */" },
});

// Ship each bundled package's actual MIT notice beside the generated asset.
// Both files enter the existing web/pkg artifact inventory and hash validation.
const packages = new Map();
for (const input of Object.keys(result.metafile.inputs)) {
  if (!input.split(/[\\/]/).includes("node_modules")) continue;
  let directory = path.dirname(path.resolve(root, input));
  for (;;) {
    let metadata;
    try { metadata = JSON.parse(await readFile(path.join(directory, "package.json"), "utf8")); }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    if (metadata?.name) {
      assert.equal(metadata.license, "MIT", `review changed license for ${metadata.name}`);
      const notice = await readFile(path.join(directory, "LICENSE"), "utf8");
      packages.set(`${metadata.name}@${metadata.version}`, notice.trim());
      break;
    }
    const parent = path.dirname(directory);
    assert.notEqual(parent, directory, `missing package metadata for ${input}`);
    directory = parent;
  }
}
assert.ok(packages.size > 0);
await writeFile(path.join(output, "python-editor-licenses.txt"),
  [...packages].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
    .map(([name, notice]) => `${name}\n${notice}`).join("\n\n") + "\n");
console.log(`Bundled Python editor dependencies with ${packages.size} license notices.`);
