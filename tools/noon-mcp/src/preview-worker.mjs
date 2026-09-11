import assert from "node:assert/strict";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { readFile, realpath, stat } from "node:fs/promises";
import path from "node:path";

import { installPinnedPyodideRoute } from "./preview-pyodide.mjs";

const MAX_REQUEST_BYTES = 8 * 1024 * 1024;
const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
const MAX_SOURCE_BYTES = 1_000_000;
const MAX_TIME_SECONDS = 600;
const MAX_PNG_BYTES = 4 * 1024 * 1024;
const webRoot = await realpath(process.env.NOON_WEB_ROOT || "/noon/web");
const require = createRequire("/opt/noon-runner/loader.cjs");
const { chromium } = require("/opt/noon-runner/playwright");

function mime(filename) {
  if (filename.endsWith(".html")) return "text/html; charset=utf-8";
  if (filename.endsWith(".js") || filename.endsWith(".mjs")) return "text/javascript; charset=utf-8";
  if (filename.endsWith(".json")) return "application/json; charset=utf-8";
  if (filename.endsWith(".wasm")) return "application/wasm";
  if (filename.endsWith(".py")) return "text/plain; charset=utf-8";
  if (filename.endsWith(".css")) return "text/css; charset=utf-8";
  return "application/octet-stream";
}

async function serveStatic(request, response) {
  try {
    const url = new URL(request.url, "http://127.0.0.1");
    const relative = decodeURIComponent(url.pathname === "/" ? "/agent-preview-host.html" : url.pathname).replace(/^\/+/, "");
    const candidate = path.resolve(webRoot, relative);
    if (candidate !== webRoot && !candidate.startsWith(`${webRoot}${path.sep}`)) throw new Error("path escaped web root");
    const resolved = await realpath(candidate);
    if (resolved !== webRoot && !resolved.startsWith(`${webRoot}${path.sep}`)) throw new Error("symlink escaped web root");
    const metadata = await stat(resolved);
    if (!metadata.isFile()) throw new Error("not a file");
    response.writeHead(200, { "content-type": mime(resolved), "cache-control": "no-store" });
    response.end(await readFile(resolved));
  } catch {
    response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    response.end("not found");
  }
}

function fail(message) {
  throw new Error(message);
}

function requestId(value) {
  if ((typeof value !== "string" && !Number.isSafeInteger(value)) || String(value).length > 128) fail("invalid request id");
  return value;
}

function finiteTime(value) {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0 || value > MAX_TIME_SECONDS) fail("invalid preview time");
  return value;
}

function sourceText(value) {
  if (typeof value !== "string" || value.trim() === "" || value.includes("\0")) fail("invalid preview source");
  if (Buffer.byteLength(value, "utf8") > MAX_SOURCE_BYTES) fail("preview source exceeds byte limit");
  return value;
}

async function *boundedLines(stream) {
  let buffered = Buffer.alloc(0);
  for await (const chunk of stream) {
    buffered = Buffer.concat([buffered, chunk]);
    if (buffered.length > MAX_REQUEST_BYTES) fail("preview protocol request exceeded byte limit");
    while (true) {
      const newline = buffered.indexOf(0x0a);
      if (newline < 0) break;
      const line = buffered.subarray(0, newline);
      buffered = buffered.subarray(newline + 1);
      if (line.length > MAX_REQUEST_BYTES) fail("preview protocol request exceeded byte limit");
      if (line.length > 0) yield line.toString("utf8");
    }
  }
  if (buffered.length !== 0) fail("preview protocol ended with an unterminated request");
}

function send(record) {
  const line = `${JSON.stringify(record)}\n`;
  if (Buffer.byteLength(line, "utf8") > MAX_RESPONSE_BYTES) fail("preview protocol response exceeded byte limit");
  process.stdout.write(line);
}

const server = createServer((request, response) => { void serveStatic(request, response); });
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(0, "127.0.0.1", resolve);
});
const address = server.address();
assert.equal(typeof address, "object");
const hostUrl = `http://127.0.0.1:${address.port}/agent-preview-host.html`;
let browser;
let page;
let opened = false;
let lastRequestedTime = -1;
let closing = false;

async function capture(snapshot) {
  const png = await page.locator("#scene").screenshot();
  if (png.length <= 8 || png.length > MAX_PNG_BYTES || png[0] !== 137 || png[1] !== 80 || png[2] !== 78 || png[3] !== 71) {
    fail("preview screenshot was not a bounded PNG");
  }
  return {
    snapshot,
    image: { mimeType: "image/png", width: 960, height: 540, encodedBytes: png.length, base64: png.toString("base64") },
  };
}

async function handle(request) {
  if (request === null || typeof request !== "object" || Array.isArray(request)) fail("preview request must be an object");
  const id = requestId(request.id);
  const op = request.op;
  try {
    if (op === "open") {
      if (opened) fail("preview worker opens one scene");
      const source = sourceText(request.source);
      const loopDurationSeconds = request.loopDurationSeconds === undefined ? 4 : finiteTime(request.loopDurationSeconds);
      if (loopDurationSeconds <= 0) fail("loop duration must be positive");
      const snapshot = await page.evaluate(({ source, loopDurationSeconds }) =>
        window.noonAgentPreviewHost.open(source, loopDurationSeconds), { source, loopDurationSeconds });
      opened = true;
      lastRequestedTime = 0;
      send({ id, ok: true, ...(await capture(snapshot)) });
      return;
    }
    if (op === "sample") {
      if (!opened) fail("preview scene is not open");
      const time = finiteTime(request.time);
      if (time < lastRequestedTime) fail("preview worker cannot sample backwards");
      const snapshot = await page.evaluate((value) => window.noonAgentPreviewHost.sample(value), time);
      lastRequestedTime = time;
      send({ id, ok: true, ...(await capture(snapshot)) });
      return;
    }
    if (op === "inspect") {
      if (!opened) fail("preview scene is not open");
      const snapshot = await page.evaluate(() => window.noonAgentPreviewHost.status());
      send({ id, ok: true, snapshot });
      return;
    }
    if (op === "close") {
      const reason = typeof request.reason === "string" && request.reason.trim() ? request.reason.slice(0, 512) : "preview worker closed";
      const snapshot = await page.evaluate((value) => window.noonAgentPreviewHost.close(value), reason);
      send({ id, ok: true, snapshot });
      closing = true;
      return;
    }
    fail("unsupported preview operation");
  } catch (error) {
    send({ id, ok: false, error: String(error?.message ?? error).slice(0, 4096) });
  }
}

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
  page = await browser.newPage({ viewport: { width: 960, height: 540 }, deviceScaleFactor: 1 });
  await installPinnedPyodideRoute(page.context());
  await page.goto(hostUrl);
  await page.waitForFunction(() => window.noonAgentPreviewHost !== undefined);
  for await (const line of boundedLines(process.stdin)) {
    let request;
    try { request = JSON.parse(line); }
    catch { fail("preview protocol request was not valid JSON"); }
    await handle(request);
    if (closing) break;
  }
} finally {
  try { await page?.close(); } catch {}
  try { await browser?.close(); } catch {}
  await new Promise((resolve) => server.close(resolve));
}
