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
const PROVENANCE_ONLY_STATUSES = new Set(["blocked", "deferred"]);
const EXACT_SOURCE_REUSE = "source-equivalent-manim-v0.21";
const MANIM_IMPORT_PRELUDE = "from manim import *";

function assertObject(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
}

function normalizeSource(source) {
  if (typeof source !== "string") {
    throw new Error("source must be a string");
  }
  return source.replace(/\r\n?/gu, "\n").trimEnd();
}

export function normalizedSourceHash(source) {
  return createHash("sha256").update(normalizeSource(source), "utf8").digest("hex");
}

export function directiveSourceFromFixture(source) {
  const normalized = normalizeSource(source);
  const lines = normalized.split("\n");
  if (lines[0]?.trim() !== MANIM_IMPORT_PRELUDE) {
    return normalized;
  }
  lines.shift();
  while (lines[0]?.trim() === "") {
    lines.shift();
  }
  return lines.join("\n").trimEnd();
}

function isReferenceEntry(entry) {
  return typeof entry.upstream === "string" && entry.upstream.startsWith("reference/");
}

function inventoryIdentity(example) {
  return `${example.source_path}:${example.directive_line}:${example.name ?? "<unnamed>"}`;
}

function validateProvenance(value, name) {
  assertObject(value, name);
  if (typeof value.source_path !== "string" || value.source_path.length === 0) {
    throw new Error(`${name}.source_path must be a non-empty string`);
  }
  if (!Number.isInteger(value.directive_line) || value.directive_line < 1) {
    throw new Error(`${name}.directive_line must be a positive integer`);
  }
  if (value.name !== null && typeof value.name !== "string") {
    throw new Error(`${name}.name must be a string or null`);
  }
  if (typeof value.source_sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(value.source_sha256)) {
    throw new Error(`${name}.source_sha256 must be a lowercase SHA-256 digest`);
  }
}

function provenanceKey(value) {
  return JSON.stringify([
    value.source_path,
    value.directive_line,
    value.name,
    value.source_sha256,
  ]);
}

function inventoryProjection(example) {
  return {
    source_path: example.source_path,
    directive_line: example.directive_line,
    name: example.name,
    source_sha256: example.source_sha256,
  };
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
  if (lock.schema_version !== 2) {
    throw new Error(`unsupported reference coverage lock schema ${lock.schema_version}`);
  }
  if (!Number.isInteger(lock.minimum_reconciled_examples) || lock.minimum_reconciled_examples < 0) {
    throw new Error("coverage lock minimum_reconciled_examples must be a non-negative integer");
  }
  if (!Number.isInteger(lock.minimum_classified_examples) || lock.minimum_classified_examples < 0) {
    throw new Error("coverage lock minimum_classified_examples must be a non-negative integer");
  }
  if (lock.minimum_classified_examples < lock.minimum_reconciled_examples) {
    throw new Error(
      "coverage lock minimum_classified_examples must be at least minimum_reconciled_examples",
    );
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
  const byProvenance = new Map();
  for (const [index, example] of inventory.examples.entries()) {
    validateProvenance(example, `inventory.examples[${index}]`);
    const matches = byHash.get(example.source_sha256) ?? [];
    matches.push({ index, example });
    byHash.set(example.source_sha256, matches);

    const key = provenanceKey(example);
    if (byProvenance.has(key)) {
      throw new Error(`duplicate inventory provenance for ${inventoryIdentity(example)}`);
    }
    byProvenance.set(key, { index, example });
  }

  const referenceEntries = manifest.entries.filter(isReferenceEntry);
  const reconciled = [];
  const provenanceClassified = [];
  const classifiedWithoutExactSource = [];
  const matchedInventoryIndices = new Set();

  function claimInventory(index, example) {
    if (matchedInventoryIndices.has(index)) {
      throw new Error(`multiple manifest entries map to ${inventoryIdentity(example)}`);
    }
    matchedInventoryIndices.add(index);
  }

  for (const entry of referenceEntries) {
    assertObject(entry, `manifest entry ${entry?.id ?? "<unknown>"}`);
    if (!CLASSIFIED_STATUSES.has(entry.status)) {
      throw new Error(
        `reference manifest entry ${entry.id ?? "<unknown>"} has unsupported status ${entry.status ?? "<missing>"}`,
      );
    }

    const hasProvenance = entry.reference_provenance !== undefined;
    if (entry.reuse !== EXACT_SOURCE_REUSE) {
      if (!hasProvenance) {
        classifiedWithoutExactSource.push({
          id: entry.id,
          title: entry.title ?? null,
          status: entry.status,
          upstream: entry.upstream,
        });
        continue;
      }
      if (!PROVENANCE_ONLY_STATUSES.has(entry.status)) {
        throw new Error(
          `provenance-only reference entry ${entry.id} must be blocked or deferred, got ${entry.status}`,
        );
      }
      validateProvenance(entry.reference_provenance, `reference entry ${entry.id}.reference_provenance`);
      const matched = byProvenance.get(provenanceKey(entry.reference_provenance));
      if (matched === undefined) {
        throw new Error(
          `reference entry ${entry.id} provenance does not match one pinned directive`,
        );
      }
      claimInventory(matched.index, matched.example);
      provenanceClassified.push({
        id: entry.id,
        title: entry.title ?? null,
        status: entry.status,
        upstream: entry.upstream,
        inventory: inventoryProjection(matched.example),
      });
      continue;
    }

    if (hasProvenance) {
      throw new Error(
        `exact-source reference entry ${entry.id} must not also set reference_provenance`,
      );
    }
    if (typeof entry.upstream_source !== "string" || entry.upstream_source.length === 0) {
      throw new Error(`exact-source reference entry ${entry.id} is missing upstream_source`);
    }

    const fixturePath = path.resolve(repositoryRoot, entry.upstream_source);
    const fixtureSource = await readSource(fixturePath, "utf8");
    const fixtureHash = normalizedSourceHash(directiveSourceFromFixture(fixtureSource));
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
    claimInventory(index, example);
    reconciled.push({
      id: entry.id,
      title: entry.title ?? null,
      status: entry.status,
      upstream: entry.upstream,
      upstream_source: entry.upstream_source,
      inventory: inventoryProjection(example),
    });
  }

  if (reconciled.length < lock.minimum_reconciled_examples) {
    throw new Error(
      `reference coverage regression: expected at least ${lock.minimum_reconciled_examples} reconciled examples, found ${reconciled.length}`,
    );
  }
  if (matchedInventoryIndices.size < lock.minimum_classified_examples) {
    throw new Error(
      `reference classification regression: expected at least ${lock.minimum_classified_examples} classified examples, found ${matchedInventoryIndices.size}`,
    );
  }

  return {
    schema_version: 2,
    upstream: inventory.upstream,
    inventory_examples: inventory.examples.length,
    manifest_reference_entries: referenceEntries.length,
    reconciled_examples: reconciled.length,
    provenance_classified_examples: provenanceClassified.length,
    classified_examples: matchedInventoryIndices.size,
    classified_without_exact_source: classifiedWithoutExactSource,
    unclassified_inventory_examples: inventory.examples.length - matchedInventoryIndices.size,
    provenance_classified: provenanceClassified,
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
