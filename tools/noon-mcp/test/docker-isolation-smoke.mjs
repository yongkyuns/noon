import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { DockerIsolatedProcess, loadPreviewRuntimeConfig } from "../src/preview-isolation.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../../..");
const controller = new AbortController();
const deadline = setTimeout(() => controller.abort("Docker isolation smoke timed out"), 120_000);
let owner;
try {
  const config = await loadPreviewRuntimeConfig({
    repoRoot,
    configPath: process.env.NOON_PREVIEW_RUNTIME_CONFIG,
  });
  owner = await DockerIsolatedProcess.launch(config,
    ["node", "/noon/tools/noon-mcp/test/preview-isolation-probe.mjs"],
    { signal: controller.signal });
  let stdout = "";
  owner.stdout.setEncoding("utf8");
  owner.stdout.on("data", (chunk) => {
    stdout += chunk;
    if (stdout.length > 128 * 1024) controller.abort("Docker isolation stdout exceeded limit");
  });
  owner.stdin.end();
  const exited = await owner.exited;
  assert.equal(exited.code, 0, owner.diagnostics.stderr);
  const lines = stdout.trim().split(/\r?\n/).filter(Boolean);
  const evidence = JSON.parse(lines.at(-1));
  assert.equal(evidence.ok, true);
  assert.equal(evidence.rendererBackend, "WebGL2");
  assert.equal(evidence.networkBlocked, true);
  assert.equal(evidence.chromiumSandbox, true);
  assert.ok(evidence.pngBytes > 1000);
  console.log(JSON.stringify({ ...evidence, imageId: config.imageId, containerId: owner.containerId }));
} finally {
  clearTimeout(deadline);
  await owner?.close("isolation smoke complete");
}
