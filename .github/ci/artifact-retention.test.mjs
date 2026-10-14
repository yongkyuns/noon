import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const githubDirectory = dirname(dirname(fileURLToPath(import.meta.url)));

function* yamlFiles(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* yamlFiles(path);
    else if (/\.ya?ml$/.test(entry.name)) yield path;
  }
}

test("all CI artifact uploads explicitly expire within three days", () => {
  let uploads = 0;
  for (const path of yamlFiles(githubDirectory)) {
    const lines = readFileSync(path, "utf8").split(/\r?\n/);
    for (let index = 0; index < lines.length; index += 1) {
      const upload = lines[index].match(
        /^(\s*)(?:-\s+)?uses:\s*actions\/upload-(?:pages-)?artifact@/,
      );
      if (!upload) continue;
      uploads += 1;

      // Keep sibling step properties, but stop at the next step or parent block.
      const indentation = upload[1].length;
      let end = index + 1;
      for (; end < lines.length; end += 1) {
        const line = lines[end];
        if (!line.trim() || line.trimStart().startsWith("#")) continue;
        const depth = line.length - line.trimStart().length;
        if (depth < indentation || (depth === indentation && line.trimStart().startsWith("- "))) {
          break;
        }
      }

      const location = `${relative(githubDirectory, path)}:${index + 1}`;
      const retention = lines.slice(index + 1, end).join("\n").match(
        /^\s+retention-days:\s*(\d+)\s*(?:#.*)?$/m,
      );
      assert.ok(retention, `${location}: set an explicit retention-days value`);
      const days = Number(retention[1]);
      assert.ok(days >= 1 && days <= 3, `${location}: retention must be 1–3 days, got ${days}`);
    }
  }
  assert.ok(uploads > 0, "the policy must discover CI artifact upload steps");
});
