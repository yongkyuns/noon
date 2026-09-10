#!/usr/bin/env node
import path from "node:path";
import { fileURLToPath } from "node:url";

import { runPreviewCli } from "../src/preview-cli.mjs";
import { loadPreviewRuntimeConfig } from "../src/preview-isolation.mjs";
import { AgentPreviewService } from "../src/preview-service.mjs";

const USAGE = `Usage: noon-preview --source <scene.py> [--output <dir>] [--loop-duration <seconds>] [--time <seconds> ...]\n\nRenders through the same isolated preview service used by Noon MCP, including retained PNG provenance. Runtime selection comes from NOON_PREVIEW_RUNTIME_CONFIG or the trusted default created by setup-preview-runtime.mjs.\n`;

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(USAGE);
    return;
  }
  const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const repoRoot = path.resolve(packageRoot, "../..");
  const config = await loadPreviewRuntimeConfig({
    repoRoot,
    configPath: process.env.NOON_PREVIEW_RUNTIME_CONFIG,
  });
  const result = await runPreviewCli({
    argv,
    serviceFactory: () => new AgentPreviewService({ isolationConfig: config }),
  });
  process.stdout.write(`${JSON.stringify({ outputDir: result.outputDir, manifestPath: result.manifestPath, samples: result.manifest.samples.length })}\n`);
}

main().catch((error) => {
  process.stderr.write(`${String(error?.stack ?? error)}\n`);
  process.exitCode = 1;
});
