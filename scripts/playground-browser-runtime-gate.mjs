import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import playwright from "playwright";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const browserName = process.env.NOON_PLAYGROUND_BROWSER ?? "chromium";
const profileName = process.env.NOON_PLAYGROUND_PROFILE ?? "desktop-dpr1";
const artifactDir = path.resolve(
  repoRoot,
  process.env.NOON_PLAYGROUND_MATRIX_ARTIFACTS ??
    `browser-smoke-artifacts/playground-matrix/${browserName}-${profileName}`,
);

assert.ok(
  ["chromium", "firefox", "webkit"].includes(browserName),
  `unknown playground browser: ${browserName}`,
);

function launchOptions() {
  if (browserName === "chromium") {
    return {
      headless: true,
      args: [
        "--disable-features=WebGPU",
        "--enable-unsafe-swiftshader",
        "--ignore-gpu-blocklist",
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--disable-gpu-sandbox",
        "--disable-dev-shm-usage",
      ],
    };
  }
  if (browserName === "firefox") {
    return {
      headless: true,
      firefoxUserPrefs: {
        "webgl.disabled": false,
        "webgl.force-enabled": true,
      },
    };
  }
  return { headless: true };
}

async function probe(page) {
  return page.evaluate(async () => {
    let offscreenWebgl2 = false;
    let transferredWorkerWebgl2 = false;
    let transferredWorkerWebgl2Error = "";

    if (typeof OffscreenCanvas === "function") {
      try {
        offscreenWebgl2 = new OffscreenCanvas(2, 2).getContext("webgl2") !== null;
      } catch {
        offscreenWebgl2 = false;
      }
    }

    const canTransferToWorker =
      typeof Worker === "function" &&
      typeof HTMLCanvasElement?.prototype?.transferControlToOffscreen === "function";
    if (canTransferToWorker) {
      const source = `
        self.onmessage = (event) => {
          try {
            self.postMessage({ ok: event.data.canvas.getContext("webgl2") !== null, error: "" });
          } catch (error) {
            self.postMessage({ ok: false, error: String(error) });
          }
        };
      `;
      const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
      const worker = new Worker(url);
      try {
        const htmlCanvas = document.createElement("canvas");
        htmlCanvas.width = 2;
        htmlCanvas.height = 2;
        const offscreen = htmlCanvas.transferControlToOffscreen();
        const result = await new Promise((resolve) => {
          const timeout = setTimeout(
            () => resolve({ ok: false, error: "worker WebGL2 probe timed out" }),
            5000,
          );
          worker.onmessage = (event) => {
            clearTimeout(timeout);
            resolve(event.data);
          };
          worker.onerror = (event) => {
            clearTimeout(timeout);
            resolve({ ok: false, error: event.message || "worker WebGL2 probe crashed" });
          };
          worker.postMessage({ canvas: offscreen }, [offscreen]);
        });
        transferredWorkerWebgl2 = result?.ok === true;
        transferredWorkerWebgl2Error = typeof result?.error === "string" ? result.error : "";
      } catch (error) {
        transferredWorkerWebgl2Error = String(error);
      } finally {
        worker.terminate();
        URL.revokeObjectURL(url);
      }
    }

    return {
      webAssembly: typeof WebAssembly === "object",
      worker: typeof Worker === "function",
      offscreenCanvas: typeof OffscreenCanvas === "function",
      transferControlToOffscreen:
        typeof HTMLCanvasElement?.prototype?.transferControlToOffscreen === "function",
      offscreenWebgl2,
      transferredWorkerWebgl2,
      transferredWorkerWebgl2Error,
      webgpu: typeof navigator.gpu !== "undefined",
      userAgent: navigator.userAgent,
    };
  });
}

function support(capabilities) {
  const missingBase = [
    ["WebAssembly", capabilities.webAssembly],
    ["Worker", capabilities.worker],
    ["OffscreenCanvas", capabilities.offscreenCanvas],
    ["transferControlToOffscreen", capabilities.transferControlToOffscreen],
  ]
    .filter(([, available]) => !available)
    .map(([name]) => name);
  const usableRenderHost =
    capabilities.transferredWorkerWebgl2 || capabilities.offscreenWebgl2;
  return {
    supported: missingBase.length === 0 && usableRenderHost,
    missingBase,
    usableRenderHost,
  };
}

await mkdir(artifactDir, { recursive: true });
let browser = null;
try {
  browser = await playwright[browserName].launch(launchOptions());
  const page = await browser.newPage();
  const capabilities = await probe(page);
  const result = support(capabilities);
  await writeFile(
    path.join(artifactDir, "runtime-support.json"),
    `${JSON.stringify({ browser: browserName, profile: profileName, capabilities, ...result }, null, 2)}\n`,
    "utf8",
  );

  if (!result.supported) {
    const reasons = [
      ...result.missingBase,
      ...(result.usableRenderHost ? [] : ["worker or main-thread OffscreenCanvas WebGL2"]),
    ];
    console.log(
      `↷ ${browserName}/${profileName}: runtime unsupported by available render hosts (${reasons.join(", ")})`,
    );
    process.exitCode = 0;
  } else {
    const child = spawn(process.execPath, [path.join(scriptDir, "playground-browser-matrix-smoke.mjs")], {
      cwd: repoRoot,
      env: process.env,
      stdio: "inherit",
    });
    const exitCode = await new Promise((resolve, reject) => {
      child.once("error", reject);
      child.once("exit", (code, signal) => {
        if (signal !== null) reject(new Error(`playground runtime smoke exited via ${signal}`));
        else resolve(code ?? 1);
      });
    });
    process.exitCode = exitCode;
  }
} finally {
  await browser?.close();
}
