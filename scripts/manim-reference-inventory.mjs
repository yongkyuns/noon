import { createHash } from "node:crypto";
import { execFile } from "node:child_process";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { pathToFileURL } from "node:url";

const SOURCE_EXTENSIONS = new Set([".py", ".rst"]);
const INVENTORY_ROOTS = ["docs/source", "manim"];
const IGNORED_DIRECTORIES = new Set([".git", ".venv", "__pycache__", "node_modules"]);
export const PINNED_MANIM_VERSION = "v0.21.0";
export const PINNED_MANIM_REVISION = "861cd4849b17db1db3515b531ffe80b297848f93";
const execFileAsync = promisify(execFile);

function leadingWhitespace(line) {
  return line.match(/^\s*/u)?.[0].length ?? 0;
}

function compareCodePoints(left, right) {
  if (left < right) return -1;
  if (left > right) return 1;
  return 0;
}

function dedent(lines) {
  const nonBlank = lines.filter((line) => line.trim().length > 0);
  if (nonBlank.length === 0) {
    return "";
  }
  const indent = Math.min(...nonBlank.map(leadingWhitespace));
  return lines
    .map((line) => (line.trim().length === 0 ? "" : line.slice(indent)))
    .join("\n")
    .trimEnd();
}

function sourceHash(source) {
  return createHash("sha256").update(source, "utf8").digest("hex");
}

export function extractManimDirectives(text, sourcePath) {
  const lines = text.replace(/\r\n?/gu, "\n").split("\n");
  const examples = [];

  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^(\s*)\.\.\s+manim::(?:\s+(.*?))?\s*$/u);
    if (!match) {
      continue;
    }

    const directiveIndent = match[1].length;
    const name = match[2]?.trim() || null;
    const body = [];
    let cursor = index + 1;
    while (cursor < lines.length) {
      const line = lines[cursor];
      if (line.trim().length === 0) {
        body.push(line);
        cursor += 1;
        continue;
      }
      if (leadingWhitespace(line) <= directiveIndent) {
        break;
      }
      body.push(line);
      cursor += 1;
    }

    const options = {};
    const sourceLines = [];
    let readingOptions = true;
    for (const line of body) {
      const trimmed = line.trim();
      if (readingOptions && trimmed.length === 0) {
        continue;
      }
      const option = readingOptions ? trimmed.match(/^:([\w-]+):(?:\s*(.*))?$/u) : null;
      if (option) {
        options[option[1]] = option[2]?.trim() || true;
        continue;
      }
      readingOptions = false;
      sourceLines.push(line);
    }

    while (sourceLines.length > 0 && sourceLines[0].trim().length === 0) {
      sourceLines.shift();
    }
    const source = dedent(sourceLines);
    examples.push({
      name,
      source_path: sourcePath,
      directive_line: index + 1,
      options,
      source,
      source_sha256: sourceHash(source),
    });
    index = cursor - 1;
  }

  return examples;
}

async function collectSourceFiles(repositoryRoot) {
  const files = [];
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => compareCodePoints(left.name, right.name));
    for (const entry of entries) {
      if (entry.isDirectory()) {
        if (!IGNORED_DIRECTORIES.has(entry.name)) {
          await visit(path.join(directory, entry.name));
        }
        continue;
      }
      if (entry.isFile() && SOURCE_EXTENSIONS.has(path.extname(entry.name))) {
        files.push(path.join(directory, entry.name));
      }
    }
  }
  for (const relativeRoot of INVENTORY_ROOTS) {
    await visit(path.join(repositoryRoot, relativeRoot));
  }
  return files;
}

export function validatePinnedRevision(revision) {
  if (revision !== PINNED_MANIM_REVISION) {
    throw new Error(
      `Manim reference inventory requires ${PINNED_MANIM_VERSION} commit ${PINNED_MANIM_REVISION}, got ${revision || "an unknown revision"}`,
    );
  }
  return revision;
}

async function resolvePinnedRevision(repositoryRoot) {
  let stdout;
  try {
    ({ stdout } = await execFileAsync(
      "git",
      ["-C", repositoryRoot, "rev-parse", "HEAD"],
      { encoding: "utf8" },
    ));
  } catch (error) {
    throw new Error(
      `Manim reference inventory requires a git checkout at ${PINNED_MANIM_VERSION} commit ${PINNED_MANIM_REVISION}`,
      { cause: error },
    );
  }
  return validatePinnedRevision(stdout.trim());
}

export async function buildReferenceInventory(repositoryRoot, { revision = null } = {}) {
  const absoluteRoot = path.resolve(repositoryRoot);
  const pinnedRevision =
    revision === null ? await resolvePinnedRevision(absoluteRoot) : validatePinnedRevision(revision);
  const examples = [];
  for (const file of await collectSourceFiles(absoluteRoot)) {
    const relativePath = path.relative(absoluteRoot, file).split(path.sep).join("/");
    const text = await readFile(file, "utf8");
    examples.push(...extractManimDirectives(text, relativePath));
  }
  examples.sort(
    (left, right) =>
      compareCodePoints(left.source_path, right.source_path) ||
      left.directive_line - right.directive_line ||
      compareCodePoints(left.name ?? "", right.name ?? ""),
  );
  return {
    schema_version: 1,
    upstream: {
      repository: "ManimCommunity/manim",
      version: PINNED_MANIM_VERSION,
      revision: pinnedRevision,
    },
    scanned_roots: INVENTORY_ROOTS,
    examples,
  };
}

async function main() {
  const root = process.argv[2];
  if (!root) {
    throw new Error("usage: node scripts/manim-reference-inventory.mjs MANIM_REPOSITORY_ROOT");
  }
  process.stdout.write(`${JSON.stringify(await buildReferenceInventory(root), null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
