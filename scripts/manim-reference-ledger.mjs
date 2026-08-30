import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

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

export function referenceInventoryFingerprint(examples) {
  if (!Array.isArray(examples)) {
    throw new Error("inventory.examples must be an array");
  }
  const projection = examples.map((example, index) => {
    validateExampleShape(example, `inventory.examples[${index}]`);
    return {
      source_path: example.source_path,
      directive_line: example.directive_line,
      name: example.name,
      source_sha256: example.source_sha256,
    };
  });
  return createHash("sha256").update(JSON.stringify(projection), "utf8").digest("hex");
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
  if (!Number.isInteger(ledger.example_count) || ledger.example_count < 0) {
    throw new Error("ledger.example_count must be a non-negative integer");
  }
  if (typeof ledger.provenance_sha256 !== "string" || !/^[0-9a-f]{64}$/u.test(ledger.provenance_sha256)) {
    throw new Error("ledger.provenance_sha256 must be a lowercase SHA-256 digest");
  }
  if (!Array.isArray(inventory.examples)) {
    throw new Error("inventory.examples must be an array");
  }
  if (inventory.examples.length !== ledger.example_count) {
    throw new Error(
      `reference example count drift: expected ${ledger.example_count}, extracted ${inventory.examples.length}`,
    );
  }

  const fingerprint = referenceInventoryFingerprint(inventory.examples);
  if (fingerprint !== ledger.provenance_sha256) {
    throw new Error(
      `reference inventory provenance drift: expected ${ledger.provenance_sha256}, extracted ${fingerprint}`,
    );
  }

  return {
    schema_version: 1,
    upstream: inventory.upstream,
    tracked_examples: inventory.examples.length,
    provenance_sha256: fingerprint,
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
