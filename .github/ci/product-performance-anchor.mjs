import assert from "node:assert/strict";
import { appendFile, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { productMeasurement } from "../../scripts/playground-product-fps.mjs";

export function validateProductPerformanceAnchor(value) {
  assert.equal(value?.schemaVersion, 1, "unsupported product performance anchor schema");
  assert.match(value?.source ?? "", /^[0-9a-f]{40}$/, "product performance anchor source must be a commit SHA");
  assert.ok(typeof value?.label === "string" && value.label.length > 0,
    "product performance anchor must have a label");
  assert.equal(value?.policy, "same-run-alternating-pairs",
    "product performance anchor must use the same-run alternating-pair policy");
  assert.deepEqual(value?.workloads,
    ["parity-square-and-circle", "showcase-camera-follows-path",
      "showcase-first-scene", "showcase-raster-images", "showcase-bezier-paths"],
    "product performance anchor workloads changed");
  return value;
}

export async function readProductPerformanceAnchor() {
  const manifest = fileURLToPath(new URL("./product-performance-anchor.json", import.meta.url));
  return validateProductPerformanceAnchor(JSON.parse(await readFile(manifest, "utf8")));
}

async function main() {
  assert.ok(process.argv.length === 3 && ["pin", "cohorts"].includes(process.argv[2]), "expected pin or cohorts");
  const anchor = await readProductPerformanceAnchor();
  if (process.argv[2] === "cohorts") {
    for (const example of anchor.workloads) {
      productMeasurement(example); // Reject an unimplemented protocol before invoking any browser.
      const directory = example === "parity-square-and-circle" ? "."
        : example === "showcase-camera-follows-path" ? "camera" : example;
      console.log(`${example}\t${directory}`);
    }
    return;
  }
  assert.ok(process.env.GITHUB_ENV && process.env.GITHUB_OUTPUT,
    "pinning the product performance anchor requires GitHub output files");
  await appendFile(process.env.GITHUB_ENV, `NOON_PRODUCT_ANCHOR_SHA=${anchor.source}\n`);
  await appendFile(process.env.GITHUB_OUTPUT,
    `anchor-sha=${anchor.source}\nanchor-label=${anchor.label}\n`);
  console.log(`Pinned cumulative product anchor ${anchor.label}: ${anchor.source}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}
