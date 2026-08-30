import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

function locationKey(example) {
  return `${example.source_path}:${example.directive_line}`;
}

function assertObject(value, name) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${name} must be an object`);
  }
}

function validateExampleShape(example, name) {
  assertObject(example, name);
  if (typeof example.source_path !== "string" || example.source_path.length === 0) {
    throw new Error(`${name}.source_path must be a non-empty string`);
  }
  if (!Number.isInteger(example.directive_line) || example.directive_line < 1) {
    throw new Error(`${name}.directive_line must be a positive integer`);
  }
  if (example.name !== null && typeof example.name !== "string") {
    throw new Error(`${name}.name must be a string or null`);
  }
  if (typeof example.source_sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(example.source_sha256)) {
    throw new Error(`${name}.source_sha256 must be a lowercase SHA-256 digest`);
  }
}

export function validateReferenceLedger(inventory, ledger) {
  assertObject(inventory, "inventory");
  assertObject(ledger, "ledger");
  if (inventory.schema_version !== 1) {
    throw new Error(`unsupported inventory schema ${inventory.schema_version}`);
  }
  if (ledger.schema_version !== 1) {
    throw new Error(`unsupported ledger schema ${ledger.schema_version}`);
  }
  if (JSON.stringify(ledger.upstream) !== JSON.stringify(inventory.upstream)) {
    throw new Error("ledger upstream provenance does not match extracted inventory");
  }
  if (!Array.isArray(inventory.examples) || !Array.isArray(ledger.entries)) {
    throw new Error("inventory.examples and ledger.entries must be arrays");
  }

  const extracted = new Map();
  for (const [index, example] of inventory.examples.entries()) {
    validateExampleShape(example, `inventory.examples[${index}]`);
    const key = locationKey(example);
    if (extracted.has(key)) {
      throw new Error(`duplicate extracted reference location ${key}`);
    }
    extracted.set(key, example);
  }

  const tracked = new Set();
  for (const [index, entry] of ledger.entries.entries()) {
    validateExampleShape(entry, `ledger.entries[${index}]`);
    const key = locationKey(entry);
    if (tracked.has(key)) {
      throw new Error(`duplicate ledger reference location ${key}`);
    }
    tracked.add(key);
    const example = extracted.get(key);
    if (!example) {
      throw new Error(`ledger contains reference absent from pinned inventory: ${key}`);
    }
    if (entry.name !== example.name || entry.source_sha256 !== example.source_sha256) {
      throw new Error(`ledger provenance drift at ${key}`);
    }
  }

  const missing = [...extracted.keys()].filter((key) => !tracked.has(key));
  if (missing.length > 0) {
    throw new Error(`ledger is missing ${missing.length} pinned reference example(s): ${missing.slice(0, 5).join(", ")}`);
  }

  return {
    schema_version: 1,
    upstream: inventory.upstream,
    tracked_examples: tracked.size,
  };
}

async function main() {
  const [inventoryPath, ledgerPath] = process.argv.slice(2);
  if (!inventoryPath || !ledgerPath) {
    throw new Error("usage: node scripts/manim-reference-ledger.mjs INVENTORY_JSON LEDGER_JSON");
  }
  const [inventory, ledger] = await Promise.all(
    [inventoryPath, ledgerPath].map(async (file) => JSON.parse(await readFile(file, "utf8"))),
  );
  process.stdout.write(`${JSON.stringify(validateReferenceLedger(inventory, ledger), null, 2)}\n`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await main();
}
