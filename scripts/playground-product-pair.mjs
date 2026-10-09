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
  headless: false, // #1933 headed standard macOS; headless falls back to SwiftShader.
  args: browserArgs("webgl", { gpuMode: "hardware" }),
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

// #1933 diagnostic-only: actual unmasked Metal-backed WebGL2, not a
// trust-the-flags check. One exclusive proof BEFORE either original scored side.
async function proveAppleParavirtualRenderer() {
  const browser = await chromium.connect(browserServer.wsEndpoint());
  try {
    assert.equal(browser.version(), "151.0.7922.34", "pinned Chromium changed");
    const context = await browser.newContext({ viewport: { width: 64, height: 64 } });
    try {
      const page = await context.newPage();
      await page.goto("about:blank");
      const proof = await page.evaluate(() => {
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 64;
        const gl = canvas.getContext("webgl2", {
          preserveDrawingBuffer: true, antialias: false, powerPreference: "high-performance",
        });
        if (!gl) return { backend: "missing", unmaskedRenderer: "", unmaskedVendor: "",
          clearPixel: null, trianglePixel: null, shaderLinked: false,
          glError: null, contextLost: null };
        const dbg = gl.getExtension("WEBGL_debug_renderer_info");
        const unmaskedVendor = dbg ? String(gl.getParameter(dbg.UNMASKED_VENDOR_WEBGL)) : "";
        const unmaskedRenderer = dbg ? String(gl.getParameter(dbg.UNMASKED_RENDERER_WEBGL)) : "";
        gl.viewport(0, 0, 64, 64);
        gl.clearColor(0.2, 0.4, 0.6, 1); gl.clear(gl.COLOR_BUFFER_BIT);
        const pixel = new Uint8Array(4);
        gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
        const clearPixel = [...pixel];
        const makeShader = (kind, source) => {
          const s = gl.createShader(kind); gl.shaderSource(s, source); gl.compileShader(s);
          return { s, ok: Boolean(gl.getShaderParameter(s, gl.COMPILE_STATUS)) };
        };
        const vs = makeShader(gl.VERTEX_SHADER,
          "#version 300 es\nvoid main(){vec2 p[3]=vec2[3](vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));gl_Position=vec4(p[gl_VertexID],0.,1.);}");
        const fs = makeShader(gl.FRAGMENT_SHADER,
          "#version 300 es\nprecision highp float;\nout vec4 color;\nvoid main(){color=vec4(0.6,0.2,0.4,1.);}");
        let shaderLinked = false, trianglePixel = null;
        if (vs.ok && fs.ok) {
          const program = gl.createProgram();
          gl.attachShader(program, vs.s); gl.attachShader(program, fs.s);
          gl.linkProgram(program);
          shaderLinked = Boolean(gl.getProgramParameter(program, gl.LINK_STATUS));
          if (shaderLinked) {
            gl.useProgram(program); gl.drawArrays(gl.TRIANGLES, 0, 3);
            gl.readPixels(32, 32, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel);
            trianglePixel = [...pixel];
          }
        }
        return { backend: "WebGL2", unmaskedVendor, unmaskedRenderer, clearPixel,
          trianglePixel, shaderLinked, glError: gl.getError(), contextLost: gl.isContextLost() };
      });
      proof.browserVersion = browser.version();
      proof.pairIndex = pairIndex;
      proof.exampleId = exampleId;
      proof.platform = process.platform;
      proof.arch = process.arch;
      proof.diagnosticOnly = true;
      proof.qualification = false;
      proof.performanceAcceptance = false;
      proof.mergeApproval = false;
      await writeFile(path.join(evidenceRoot, "macos-renderer-proof-" + pairIndex + ".json"),
        JSON.stringify(proof, null, 2) + "\n", { flag: "wx" });
      assert.equal(proof.platform, "darwin", "nonmacOS host");
      assert.equal(proof.arch, "arm64", "not standard hosted Apple Silicon arm64");
      assert.equal(proof.backend, "WebGL2", "WebGL2 unavailable");
      assert.match(proof.unmaskedRenderer, /Apple.*Metal Renderer.*Apple Paravirtual/i,
        "actual Apple Paravirtual Metal renderer not observed");
      assert.ok(!/swiftshader|llvmpipe|software/i.test(proof.unmaskedRenderer),
        "software fallback is not eligible");
      const validPixel = (p, expected) =>
        Array.isArray(p) && expected.every((n, i) => Math.abs(p[i] - n) <= 3);
      assert.ok(validPixel(proof.clearPixel, [51, 102, 153, 255]),
        "clear/readPixels invalid");
      assert.ok(validPixel(proof.trianglePixel, [153, 51, 102, 255]),
        "triangle/readPixels invalid");
      assert.equal(proof.shaderLinked, true, "GLSL did not link");
      assert.equal(proof.glError, 0, "GL error");
      assert.equal(proof.contextLost, false, "GL context lost");
    } finally { await context.close(); }
  } finally { await browser.close(); }
}

try {
  await proveAppleParavirtualRenderer();
  for (const [position, role] of logicalOrder.entries()) {
    await runSide(role, position + 1);
  }
} finally {
  await browserServer.close();
}
