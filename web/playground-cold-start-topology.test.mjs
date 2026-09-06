import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

class FakeCanvas {
  clientWidth = 640;
  clientHeight = 360;
  width = 640;
  height = 360;

  transferControlToOffscreen() {
    return { width: this.width, height: this.height };
  }
}

class FakeMessageChannel {
  constructor() {
    this.port1 = {};
    this.port2 = {};
  }
}

class FakeWorker {
  static instances = [];

  constructor(url, options = {}) {
    this.url = String(url);
    this.name = options.name ?? "";
    FakeWorker.instances.push(this);
  }

  addEventListener() {}
  postMessage() {}
  terminate() {}
}

globalThis.HTMLCanvasElement = FakeCanvas;
globalThis.MessageChannel = FakeMessageChannel;
globalThis.Worker = FakeWorker;
globalThis.window = { devicePixelRatio: 1 };

const [{ PythonAuthoringClient }, { AuthoringExecutionClient }] = await Promise.all([
  import("./authoring-client.js"),
  import("./authoring-execution-client.js"),
]);

const authoring = new PythonAuthoringClient();
authoring.ready().catch(() => {});
const execution = new AuthoringExecutionClient(new FakeCanvas());
execution
  .start(JSON.stringify({ version: 1, objects: [], tracks: [] }), {
    transportMode: "transferable",
  })
  .catch(() => {});

const coldWorkers = FakeWorker.instances.map(({ url, name }) => ({
  url: new URL(url),
  name,
}));

assert.ok(coldWorkers.length > 0, "cold Run must create at least one worker owner");
assert.ok(
  coldWorkers.length <= 3,
  `cold Run must not grow beyond the current three worker owners: ${coldWorkers
    .map(({ name, url }) => `${name || "<unnamed>"}=${url.pathname.split("/").at(-1)}`)
    .join(", ")}`,
);

const STATIC_IMPORT = /\b(?:import|export)\s+(?:[^"'()]*?\s+from\s*)?["'](\.[^"']+)["']/gu;
const DYNAMIC_IMPORT = /\bimport\s*\(\s*["'](\.[^"']+)["']\s*\)/gu;
const NOON_WASM_PATHS = ["/pkg/noon_web.js", "/pkg-authoring/noon_web.js"];

async function readWorkerSource(moduleUrl) {
  try {
    return await readFile(moduleUrl, "utf8");
  } catch (error) {
    if (error?.code !== "ENOENT" || !moduleUrl.pathname.endsWith("/python-worker.js")) {
      throw error;
    }
    return readFile(new URL("./python-worker.source.js", moduleUrl), "utf8");
  }
}

async function workerDependencyReport(entryUrl) {
  const visited = new Set();
  let sourceBytes = 0;
  let ownsNoonWasm = false;

  async function visit(moduleUrl) {
    if (visited.has(moduleUrl.href)) {
      return;
    }
    visited.add(moduleUrl.href);
    const source = await readWorkerSource(moduleUrl);
    sourceBytes += Buffer.byteLength(source, "utf8");

    const imports = [
      ...source.matchAll(STATIC_IMPORT),
      ...source.matchAll(DYNAMIC_IMPORT),
    ].map((match) => new URL(match[1], moduleUrl));
    for (const dependency of imports) {
      if (NOON_WASM_PATHS.some((path) => dependency.pathname.endsWith(path))) {
        ownsNoonWasm = true;
        continue;
      }
      if (/\.(?:m?js)$/u.test(dependency.pathname)) {
        await visit(dependency);
      }
    }
  }

  await visit(entryUrl);
  return {
    entry: entryUrl.pathname.split("/").at(-1),
    sourceBytes,
    ownsNoonWasm,
  };
}

const workerReports = await Promise.all(
  coldWorkers.map(async ({ url, name }) => ({
    name,
    ...(await workerDependencyReport(url)),
  })),
);
const noonWasmOwners = workerReports.filter(({ ownsNoonWasm }) => ownsNoonWasm);

assert.ok(
  noonWasmOwners.length <= 3,
  `cold Run must not grow beyond the current three monolithic Noon WASM owners: ${noonWasmOwners
    .map(({ entry }) => entry)
    .join(", ")}`,
);

const report = {
  workerCount: workerReports.length,
  workerEntries: workerReports.map(({ entry }) => entry),
  noonWasmOwnerCount: noonWasmOwners.length,
  noonWasmOwners: noonWasmOwners.map(({ entry }) => entry),
  workerReachableSourceBytes: workerReports.reduce((total, { sourceBytes }) => total + sourceBytes, 0),
};

assert.ok(
  report.workerReachableSourceBytes > 0,
  "cold-start topology report must include reachable worker source bytes",
);

console.log(`✓ cold-start topology ${JSON.stringify(report)}`);
