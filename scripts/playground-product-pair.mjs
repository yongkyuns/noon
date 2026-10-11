import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";
import { browserArgs } from "./manim-raster-support.mjs";
import { productPairOrder } from "./paired-product-metrics.mjs";

const { chromium } = playwright;
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const e2eScript = path.join(scriptDir, "playground-product-e2e.mjs");

function required(name) {
  const value = process.env[name]?.trim();
  assert.ok(value, `missing ${name}`);
  return value;
}

const referenceRoot = path.resolve(required("NOON_PRODUCT_REFERENCE_ROOT"));
const candidateRoot = path.resolve(required("NOON_PRODUCT_CANDIDATE_ROOT"));
const evidenceRoot = path.resolve(required("NOON_PRODUCT_EVIDENCE_ROOT"));
const exampleId = required("NOON_PRODUCT_EXAMPLE");
const referenceArtifactRole = process.env.NOON_PRODUCT_REFERENCE_ARTIFACT_ROLE?.trim() || "baseline";
assert.ok(new Set(["baseline", "anchor"]).has(referenceArtifactRole),
  "NOON_PRODUCT_REFERENCE_ARTIFACT_ROLE must be baseline or anchor");
const pairIndex = Number(required("NOON_PRODUCT_PAIR_INDEX"));
assert.ok(Number.isSafeInteger(pairIndex) && pairIndex >= 1, "pair index must be a positive integer");
const port = process.env.NOON_PRODUCT_PORT?.trim() || "4205";

const logicalOrder = productPairOrder(pairIndex);
const browserServer = await chromium.launchServer({
  channel: "chromium",
  headless: true,
  args: browserArgs("webgl"),
});

async function runSide(role, position) {
  const reference = role === "baseline";
  const siteRoot = reference ? referenceRoot : candidateRoot;
  const artifactRole = reference ? referenceArtifactRole : "candidate";
  const artifactDir = path.join(evidenceRoot, artifactRole, `trial-${pairIndex}`);
  const env = {
    ...process.env,
    NOON_PRODUCT_SITE_ROOT: siteRoot,
    NOON_PRODUCT_PORT: port,
    NOON_PRODUCT_EXAMPLE: exampleId,
    NOON_PRODUCT_LABEL: role,
    NOON_PRODUCT_PAIR_INDEX: String(pairIndex),
    NOON_PRODUCT_PAIR_POSITION: String(position),
    NOON_PRODUCT_ARTIFACT_DIR: artifactDir,
    NOON_PRODUCT_BROWSER_WS_ENDPOINT: browserServer.wsEndpoint(),
  };
  await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [e2eScript], { env, stdio: "inherit" });
    let timedOut = false;
    // Fail closed on a hung trial instead of holding the entire 90-minute
    // qualification job. This watchdog never retries or replaces a sample.
    const watchdog = setTimeout(() => {
      timedOut = true;
      child.kill("SIGTERM");
    }, 180_000);
    child.once("error", (error) => {
      clearTimeout(watchdog);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(watchdog);
      if (timedOut) {
        reject(new Error(`product ${role} did not exit within 180s in pair ${pairIndex}`));
      } else if (code === 0) {
        resolve();
      } else {
        reject(new Error(`product ${role} failed for pair ${pairIndex}: ${signal ?? `exit ${code}`}`));
      }
    });
  });
}

try {
  for (const [position, role] of logicalOrder.entries()) {
    await runSide(role, position + 1);
  }
} finally {
  await browserServer.close();
}
