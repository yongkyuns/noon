import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
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
  headless: false, // #1933 Mesa GL was qualified with Xvfb headed Chrome.
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

async function requireActualMesaWebgl() {
  // The entire logical pair shares the same Chromium GPU process. Confirm the
  // ACTUAL unmasked renderer and readback once before any scored participant.
  const client = await chromium.connect(browserServer.wsEndpoint());
  try {
    const context = await client.newContext({ viewport: { width: 64, height: 64 } });
    try {
      const page = await context.newPage();
      await page.goto("about:blank");
      const proof = await page.evaluate(() => {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 64;
        const gl = canvas.getContext("webgl2", { antialias: false, preserveDrawingBuffer: true });
        if (!gl) return { backend: "missing", unmaskedRenderer: "", readbackValid: false };
        const debug = gl.getExtension("WEBGL_debug_renderer_info");
        const unmaskedRenderer = debug ? String(gl.getParameter(debug.UNMASKED_RENDERER_WEBGL)) : "";
        gl.clearColor(0.2, 0.4, 0.6, 1);
        gl.clear(gl.COLOR_BUFFER_BIT);
        const color = new Uint8Array(4);
        gl.readPixels(16, 16, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, color);
        const target = [51, 102, 153, 255];
        const readbackValid = [...color].every((c, i) => Math.abs(c - target[i]) <= 3)
          && gl.getError() === gl.NO_ERROR && !gl.isContextLost();
        return { backend: "WebGL2", unmaskedRenderer, readbackValid, pixel: [...color] };
      });
      proof.browserVersion = client.version();
      proof.pairIndex = pairIndex;
      proof.exampleId = exampleId;
      proof.lpNumThreadsConfigured = process.env.LP_NUM_THREADS || null;
      proof.galliumDriverConfigured = process.env.GALLIUM_DRIVER || null;
      // Preserve the original observed renderer even on a non-Mesa fallback.
      await writeFile(path.join(evidenceRoot, `mesa-renderer-proof-${pairIndex}.json`),
        JSON.stringify(proof, null, 2) + "\n", { flag: "wx" });
      assert.equal(proof.backend, "WebGL2", "diagnostic Mesa study needs actual WebGL2");
      assert.match(proof.unmaskedRenderer, /llvmpipe/i,
        "renderer silently switched away from pinned Mesa LLVMpipe");
      assert.ok(!/swiftshader/i.test(proof.unmaskedRenderer), "unexpected SwiftShader fallback");
      assert.equal(proof.readbackValid, true, "Mesa GL pixel readback failed");
      assert.equal(proof.browserVersion, "151.0.7922.34", "Chromium revision changed");
      assert.equal(proof.lpNumThreadsConfigured, "2", "Mesa LP_NUM_THREADS setting changed");
      assert.equal(proof.galliumDriverConfigured, "llvmpipe", "Mesa GALLIUM_DRIVER changed");
    } finally { await context.close(); }
  } finally { await client.close(); }
}

try {
  await requireActualMesaWebgl();
  for (const [position, role] of logicalOrder.entries()) {
    await runSide(role, position + 1);
  }
} finally {
  await browserServer.close();
}
