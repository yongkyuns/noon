import { readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

export const PINNED_PYODIDE_VERSION = "314.0.5";
export const PYODIDE_CDN_PREFIX = `https://cdn.jsdelivr.net/pyodide/v${PINNED_PYODIDE_VERSION}/full/`;
const PYODIDE_ROOT = "/opt/noon-runner/pyodide";

function contentType(filename) {
  if (filename.endsWith(".mjs") || filename.endsWith(".js")) return "text/javascript; charset=utf-8";
  if (filename.endsWith(".wasm")) return "application/wasm";
  if (filename.endsWith(".json")) return "application/json; charset=utf-8";
  if (filename.endsWith(".zip")) return "application/zip";
  return "application/octet-stream";
}

export function pinnedPyodideRelativePath(value) {
  const url = new URL(value);
  const prefix = new URL(PYODIDE_CDN_PREFIX);
  if (url.origin !== prefix.origin || !url.pathname.startsWith(prefix.pathname)) return null;
  const relative = decodeURIComponent(url.pathname.slice(prefix.pathname.length));
  if (!relative || relative.includes("\0") || path.isAbsolute(relative)) return null;
  const normalized = path.posix.normalize(relative);
  if (normalized === ".." || normalized.startsWith("../") || normalized.includes("/../")) return null;
  return normalized;
}

async function verifyInstalledRuntime(root) {
  const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
  if (packageJson?.name !== "pyodide" || packageJson?.version !== PINNED_PYODIDE_VERSION) {
    throw new Error(`isolated Pyodide runtime must be exactly ${PINNED_PYODIDE_VERSION}`);
  }
}

/**
 * Fulfil the worker's pinned jsDelivr Pyodide URLs from the content-addressed
 * Docker image. The browser keeps --network=none; no request is allowed to
 * escape to jsDelivr at runtime. Cross-origin module/fetch semantics still need
 * the CORS header that the real CDN would have supplied.
 */
export async function installPinnedPyodideRoute(context, { root = PYODIDE_ROOT } = {}) {
  if (!context || typeof context.route !== "function") throw new TypeError("Playwright browser context is required");
  const canonicalRoot = await realpath(root);
  await verifyInstalledRuntime(canonicalRoot);
  await context.route(`${PYODIDE_CDN_PREFIX}**`, async (route) => {
    const relative = pinnedPyodideRelativePath(route.request().url());
    if (relative === null) {
      await route.abort("blockedbyclient");
      return;
    }
    try {
      const candidate = path.resolve(canonicalRoot, relative);
      const resolved = await realpath(candidate);
      if (resolved !== canonicalRoot && !resolved.startsWith(`${canonicalRoot}${path.sep}`)) {
        throw new Error("Pyodide asset escaped pinned runtime root");
      }
      const metadata = await stat(resolved);
      if (!metadata.isFile()) throw new Error("Pyodide asset is not a regular file");
      await route.fulfill({
        status: 200,
        contentType: contentType(resolved),
        headers: {
          "access-control-allow-origin": "*",
          "cache-control": "no-store",
        },
        body: await readFile(resolved),
      });
    } catch {
      await route.fulfill({
        status: 404,
        contentType: "text/plain; charset=utf-8",
        headers: { "access-control-allow-origin": "*" },
        body: "not found",
      });
    }
  });
}
