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

function assertObject(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
}

function isReferenceEntry(entry) {
  return typeof entry.upstream === "string" && entry.upstream.startsWith("reference/");
}

function hasExactSourceEvidence(entry) {
  return (
    entry.reuse === EXACT_SOURCE_REUSE &&
    typeof entry.upstream_source === "string" &&
    entry.upstream_source.length > 0 &&
    entry.reference_provenance === undefined
  );
}

function hasProvenanceEvidence(entry) {
  if (!PROVENANCE_ONLY_STATUSES.has(entry.status)) {
    return false;
  }
  const provenance = entry.reference_provenance;
  return (
    provenance !== null &&
    typeof provenance === "object" &&
    !Array.isArray(provenance) &&
    typeof provenance.source_path === "string" &&
    provenance.source_path.length > 0 &&
    Number.isInteger(provenance.directive_line) &&
    provenance.directive_line > 0 &&
    (provenance.name === null || typeof provenance.name === "string") &&
    typeof provenance.source_sha256 === "string" &&
    /^[0-9a-f]{64}$/u.test(provenance.source_sha256)
  );
}

export function validateReferenceClassificationLock(manifest, lock) {
  assertObject(manifest, "manifest");
  assertObject(lock, "coverage lock");
  if (!Array.isArray(manifest.entries)) {
    throw new Error("manifest.entries must be an array");
  }
  if (lock.schema_version !== 2) {
    throw new Error(`unsupported reference coverage lock schema ${lock.schema_version}`);
  }
  if (!Number.isInteger(lock.minimum_classified_examples) || lock.minimum_classified_examples < 0) {
    throw new Error("coverage lock minimum_classified_examples must be a non-negative integer");
  }
  if (!Array.isArray(lock.required_classified_entry_ids)) {
    throw new Error("coverage lock required_classified_entry_ids must be an array");
  }

  const requiredIds = new Set();
  for (const [index, id] of lock.required_classified_entry_ids.entries()) {
    if (typeof id !== "string" || id.length === 0) {
      throw new Error(`coverage lock required_classified_entry_ids[${index}] must be a non-empty string`);
    }
    if (requiredIds.has(id)) {
      throw new Error(`coverage lock contains duplicate required classified entry id ${id}`);
    }
    requiredIds.add(id);
  }
  if (requiredIds.size !== lock.minimum_classified_examples) {
    throw new Error(
      `coverage lock must name exactly ${lock.minimum_classified_examples} required classified entry ids, found ${requiredIds.size}`,
    );
  }

  const referenceById = new Map();
  for (const entry of manifest.entries.filter(isReferenceEntry)) {
    assertObject(entry, `manifest entry ${entry?.id ?? "<unknown>"}`);
    if (typeof entry.id !== "string" || entry.id.length === 0) {
      throw new Error("reference manifest entry id must be a non-empty string");
    }
    if (referenceById.has(entry.id)) {
      throw new Error(`duplicate reference manifest entry id ${entry.id}`);
    }
    referenceById.set(entry.id, entry);
  }

  for (const id of requiredIds) {
    const entry = referenceById.get(id);
    if (entry === undefined) {
      throw new Error(`required classified reference entry ${id} is missing`);
    }
    if (!CLASSIFIED_STATUSES.has(entry.status)) {
      throw new Error(`required classified reference entry ${id} has unsupported status ${entry.status ?? "<missing>"}`);
    }
    if (!hasExactSourceEvidence(entry) && !hasProvenanceEvidence(entry)) {
      throw new Error(`required classified reference entry ${id} no longer carries pinned classification evidence`);
    }
  }

  return {
    schema_version: 1,
    required_classified_entries: requiredIds.size,
  };
}

async function main() {
  const [manifestPath, lockPath] = process.argv.slice(2);
  if (!manifestPath || !lockPath) {
    throw new Error(
      "usage: node scripts/manim-reference-classification-lock.mjs MANIFEST_JSON COVERAGE_LOCK_JSON",
    );
  }
  const [manifest, lock] = await Promise.all(
    [manifestPath, lockPath].map(async (file) => JSON.parse(await readFile(file, "utf8"))),
  );
  const report = validateReferenceClassificationLock(manifest, lock);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
