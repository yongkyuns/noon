#!/usr/bin/env node
import { buildDistributionManifest } from "../src/distribution-manifest.mjs";

try {
  const manifest = await buildDistributionManifest();
  process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`Noon distribution manifest failed: ${String(error?.message ?? error)}\n`);
  process.exitCode = 1;
}
