import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import { mkdir, open, readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

const webRoot = await realpath(process.env.NOON_WEB_ROOT || "/noon/web");
const require = createRequire("/opt/noon-runner/loader.cjs");
const { chromium } = require("/opt/noon-runner/playwright");

function contentType(filename) {
  if (filename.endsWith(".html")) return "text/html; charset=utf-8";
  if (filename.endsWith(".js") || filename.endsWith(".mjs")) return "text/javascript; charset=utf-8";
  if (filename.endsWith(".json")) return "application/json; charset=utf-8";
  if (filename.endsWith(".wasm")) return "application/wasm";
  if (filename.endsWith(".py")) return "text/plain; charset=utf-8";
  return "application/octet-stream";
}

const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, "http://127.0.0.1");
    const relative = decodeURIComponent(url.pathname === "/" ? "/manim-raster-host.html" : url.pathname).replace(/^\/+/, "");
    const candidate = path.resolve(webRoot, relative);
    const resolved = await realpath(candidate);
    if (resolved !== webRoot && !resolved.startsWith(`${webRoot}${path.sep}`)) throw new Error("path escaped web root");
    const metadata = await stat(resolved);
    if (!metadata.isFile()) throw new Error("not a regular file");
    response.writeHead(200, { "content-type": contentType(resolved), "cache-control": "no-store" });
    response.end(await readFile(resolved));
  } catch {
    response.writeHead(404, { "content-type": "text/plain" });
    response.end("not found");
  }
});
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(0, "127.0.0.1", resolve);
});
const address = server.address();
assert.equal(typeof address, "object");
const url = `http://127.0.0.1:${address.port}/manim-raster-host.html`;

await mkdir(process.env.HOME, { recursive: true });
const status = await readFile("/proc/self/status", "utf8");
assert.match(status, /^NoNewPrivs:\s+1$/m);
assert.match(status, /^CapEff:\s+0+$/m);

for (const target of ["/noon/web/.noon-write-probe", "/noon/tools/noon-mcp/.noon-write-probe"]) {
  await assert.rejects(open(target, "w"), (error) => ["EROFS", "EACCES", "EPERM"].includes(error.code));
}
await open("/work/.noon-write-probe", "w").then((handle) => handle.close());

let networkBlocked = false;
try {
  await fetch("http://1.1.1.1/", { signal: AbortSignal.timeout(750) });
} catch {
  networkBlocked = true;
}
assert.equal(networkBlocked, true, "container unexpectedly reached an external network");

let browser;
try {
  browser = await chromium.launch({
    headless: true,
    chromiumSandbox: true,
    args: [
      "--disable-features=WebGPU",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
      "--use-gl=angle",
      "--use-angle=swiftshader",
    ],
  });
  const page = await browser.newPage({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1 });
  await page.goto(url);
  await page.waitForFunction(() => window.noonHostRaster !== undefined);
  const source = await readFile(path.join(webRoot, "python/examples/manim_parity_square_to_circle.py"), "utf8");
  const loaded = await page.evaluate((code) => window.noonHostRaster.load(code, 4), source);
  assert.equal(loaded.rendererBackend, "WebGL2");
  const png = await page.locator("#scene").screenshot();
  assert.ok(png.length > 1000, "browser did not produce a useful preview image");

  const { readdir } = await import("node:fs/promises");
  const procEntries = await readdir("/proc", { withFileTypes: true });
  const chromiumCommands = [];
  for (const entry of procEntries) {
    if (!entry.isDirectory() || !/^\d+$/.test(entry.name)) continue;
    try {
      const raw = await readFile(`/proc/${entry.name}/cmdline`);
      const command = raw.toString("utf8").replaceAll("\0", " ").trim();
      if (/chrom(e|ium)/i.test(command)) chromiumCommands.push(command);
    } catch {}
  }
  assert.ok(chromiumCommands.length > 0, "Chromium process was not observable inside the isolated PID namespace");
  assert.equal(chromiumCommands.some((command) => command.includes("--no-sandbox")), false,
    "Chromium sandbox was disabled inside the isolation boundary");

  console.log(JSON.stringify({
    ok: true,
    rendererBackend: loaded.rendererBackend,
    pngBytes: png.length,
    networkBlocked,
    chromiumSandbox: true,
    observedChromiumProcesses: chromiumCommands.length,
  }));
} finally {
  try { await browser?.close(); }
  finally { await new Promise((resolve) => server.close(resolve)); }
}
