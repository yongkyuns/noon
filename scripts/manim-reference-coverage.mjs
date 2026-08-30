import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const CLASSIFIED_STATUSES = new Set([
  "ready",
  "blocked",
  "deferred",
  "intentional-divergence",
]);
const EXACT_SOURCE_REUSE = "source-equivalent-manim-v0.21";

function assertObject(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
}

export function normalizedSourceHash(source) {
  if (typeof source !== "string") {
    throw new Error("source must be a string");
  }
  return createHash("sha256")
    .update(source.replace(/\r\n?/gu, "\n").trimEnd(), "utf8")
    .digest("hex");
}

function isReferenceEntry(entry) {
  return typeof entry.upstream === "string" && entry.upstream.startsWith("reference/");
}

function inventoryIdentity(example) {
  return `${example.source_path}:${example.directive_line}:${example.name ?? "<unnamed>"}`;
}

function validateInputs(inventory, manifest, lock) {
  assertObject(inventory, "inventory");
  assertObject(manifest, "manifest");
  assertObject(lock, "coverage lock");
  if (inventory.schema_version !== 1 || !Array.isArray(inventory.examples)) {
    throw new Error("reference inventory must use schema 1 with an examples array");
  }
  if (!Array.isArray(manifest.entries)) {
    throw new Error("manifest.entries must be an array");
  }
  if (lock.schema_version !== 1) {
    throw new Error(`unsupported reference coverage lock schema ${lock.schema_version}`);
  }
  if (!Number.isInteger(lock.minimum_reconciled_examples) || lock.minimum_reconciled_examples < 0) {
    throw new Error("coverage lock minimum_reconciled_examples must be a non-negative integer");
  }
}

export async function reconcileReferenceCoverage(
  inventory,
  manifest,
  lock,
  { readSource = readFile, repositoryRoot = process.cwd() } = {},
) {
  validateInputs(inventory, manifest, lock);

  const byHash = new Map();
  for (const [index, example] of inventory.examples.entries()) {
    if (typeof example.source_sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(example.source_sha256)) {
      throw new Error(`inventory.examples[${index}].source_sha256 is invalid`);
    }
    const matches = byHash.get(example.source_sha256) ?? [];
    matches.push({ index, example });
    byHash.set(example.source_sha256, matches);
  }

  const referenceEntries = manifest.entries.filter(isReferenceEntry);
  const reconciled = [];
  const classifiedWithoutExactSource = [];
  const matchedInventoryIndices = new Set();

  for (const entry of referenceEntries) {
    assertObject(entry, `manifest entry ${entry?.id ?? "<unknown>"}`);
    if (!CLASSIFIED_STATUSES.has(entry.status)) {
      throw new Error(
        `reference manifest entry ${entry.id ?? "<unknown>"} has unsupported status ${entry.status ?? "<missing>"}`,
      );
    }

    if (entry.reuse !== EXACT_SOURCE_REUSE) {
      classifiedWithoutExactSource.push({
        id: entry.id,
        title: entry.title ?? null,
        status: entry.status,
        upstream: entry.upstream,
      });
      continue;
    }
    if (typeof entry.upstream_source !== "string" || entry.upstream_source.length === 0) {
      throw new Error(`exact-source reference entry ${entry.id} is missing upstream_source`);
    }

    const fixturePath = path.resolve(repositoryRoot, entry.upstream_source);
    const fixtureSource = await readSource(fixturePath, "utf8");
    const fixtureHash = normalizedSourceHash(fixtureSource);
    let matches = byHash.get(fixtureHash) ?? [];
    if (matches.length > 1 && typeof entry.title === "string") {
      const named = matches.filter(({ example }) => example.name === entry.title);
      if (named.length === 1) {
        matches = named;
      }
    }
    if (matches.length !== 1) {
      const detail = matches.length === 0 ? "no extracted directive" : `${matches.length} extracted directives`;
      throw new Error(
        `reference entry ${entry.id} fixture ${entry.upstream_source} matched ${detail} by source provenance`,
      );
    }

    const [{ index, example }] = matches;
    if (matchedInventoryIndices.has(index)) {
      throw new Error(`multiple manifest entries map to ${inventoryIdentity(example)}`);
    }
    matchedInventoryIndices.add(index);
    reconciled.push({
      id: entry.id,
      title: entry.title ?? null,
      status: entry.status,
      upstream: entry.upstream,
      upstream_source: entry.upstream_source,
      inventory: {
        source_path: example.source_path,
        directive_line: example.directive_line,
        name: example.name,
        source_sha256: example.source_sha256,
      },
    });
  }

  if (reconciled.length < lock.minimum_reconciled_examples) {
    throw new Error(
      `reference coverage regression: expected at least ${lock.minimum_reconciled_examples} reconciled examples, found ${reconciled.length}`,
    );
  }

  return {
    schema_version: 1,
    upstream: inventory.upstream,
    inventory_examples: inventory.examples.length,
    manifest_reference_entries: referenceEntries.length,
    reconciled_examples: reconciled.length,
    classified_without_exact_source: classifiedWithoutExactSource,
    unclassified_inventory_examples: inventory.examples.length - matchedInventoryIndices.size,
    reconciled,
  };
}

async function main() {
  const [inventoryPath, manifestPath, lockPath, repositoryRoot = "."] = process.argv.slice(2);
  if (!inventoryPath || !manifestPath || !lockPath) {
    throw new Error(
      "usage: node scripts/manim-reference-coverage.mjs INVENTORY_JSON MANIFEST_JSON COVERAGE_LOCK_JSON [REPOSITORY_ROOT]",
    );
  }
  const [inventory, manifest, lock] = await Promise.all(
    [inventoryPath, manifestPath, lockPath].map(async (file) => JSON.parse(await readFile(file, "utf8"))),
  );
  const report = await reconcileReferenceCoverage(inventory, manifest, lock, {
    repositoryRoot: path.resolve(repositoryRoot),
  });
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
