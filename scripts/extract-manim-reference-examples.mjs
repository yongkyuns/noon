#!/usr/bin/env node
import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const MANIM_DIRECTIVE = /^(\s*)\.\.\s+manim::(?:\s+(.*?))?\s*$/;
const OPTION = /^\s+:([A-Za-z0-9_-]+):(?:\s*(.*))?$/;

export async function extractManimReferenceExamples(referenceRoot) {
  const root = path.resolve(referenceRoot);
  const files = await collectRstFiles(root);
  const examples = [];
  for (const file of files) {
    const text = await readFile(file, "utf8");
    examples.push(...extractFileExamples(text, path.relative(root, file).split(path.sep).join("/")));
  }
  return {
    schema_version: 1,
    upstream: {
      project: "Manim Community Edition",
      version: "v0.21.0",
      scope: "docs/source/reference",
    },
    examples,
  };
}

export function extractFileExamples(text, sourcePath) {
  const lines = text.replace(/\r\n/g, "\n").split("\n");
  const examples = [];
  for (let index = 0; index < lines.length; index += 1) {
    const match = MANIM_DIRECTIVE.exec(lines[index]);
    if (match === null) continue;

    const indent = match[1].length;
    const start = index;
    let end = index + 1;
    while (end < lines.length) {
      const line = lines[end];
      if (line.trim() === "") {
        end += 1;
        continue;
      }
      const lineIndent = line.length - line.trimStart().length;
      if (lineIndent <= indent) break;
      end += 1;
    }

    const blockLines = lines.slice(start, end);
    const options = {};
    for (const line of blockLines.slice(1)) {
      const option = OPTION.exec(line);
      if (option !== null) {
        options[option[1]] = (option[2] ?? "").trim();
      }
    }

    const declaredName = (match[2] ?? "").trim();
    const scene = declaredName || findSceneClass(blockLines.slice(1));
    if (!scene) {
      throw new Error(`${sourcePath}:${start + 1}: manim directive has no scene name or class`);
    }

    const canonicalBlock = blockLines.join("\n").trimEnd() + "\n";
    examples.push({
      source_path: sourcePath,
      source_line: start + 1,
      scene,
      ref_classes: parseRefClasses(options.ref_classes),
      directive_options: sortObject(options),
      source_sha256: createHash("sha256").update(canonicalBlock).digest("hex"),
    });
    index = end - 1;
  }
  return examples;
}

function findSceneClass(lines) {
  for (const line of lines) {
    const match = /^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/.exec(line);
    if (match !== null) return match[1];
  }
  return null;
}

function parseRefClasses(value) {
  if (!value) return [];
  return value.split(/\s+/).filter(Boolean);
}

function sortObject(value) {
  return Object.fromEntries(Object.entries(value).sort(([left], [right]) => left.localeCompare(right)));
}

async function collectRstFiles(root) {
  const output = [];
  await walk(root, output);
  output.sort();
  return output;
}

async function walk(directory, output) {
  let entries;
  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") {
      throw new Error(`reference root does not exist: ${directory}`);
    }
    throw error;
  }
  entries.sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of entries) {
    const resolved = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await walk(resolved, output);
    } else if (entry.isFile() && entry.name.endsWith(".rst")) {
      output.push(resolved);
    }
  }
}

async function main(argv) {
  const [referenceRoot, ...rest] = argv;
  if (!referenceRoot) {
    throw new Error(
      "usage: node scripts/extract-manim-reference-examples.mjs <manim-docs-source-reference> [--output <path>]",
    );
  }
  let outputPath = null;
  for (let index = 0; index < rest.length; index += 1) {
    if (rest[index] !== "--output" || index + 1 >= rest.length) {
      throw new Error(`unknown or incomplete argument: ${rest[index]}`);
    }
    outputPath = rest[index + 1];
    index += 1;
  }

  const inventory = await extractManimReferenceExamples(referenceRoot);
  const serialized = `${JSON.stringify(inventory, null, 2)}\n`;
  if (outputPath === null) {
    process.stdout.write(serialized);
  } else {
    await writeFile(outputPath, serialized, "utf8");
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
