#!/usr/bin/env node
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const serverPath = path.join(packageRoot, "src", "server.mjs");
const repoRoot = process.env.NOON_REPO;
const python = process.env.NOON_PYTHON;

if (!repoRoot || !path.isAbsolute(repoRoot)) throw new Error("NOON_REPO must name the trusted checkout with an absolute path");
if (!python || !path.isAbsolute(python)) throw new Error("NOON_PYTHON must name Python 3.12+ with an absolute path");
if (process.env.NOON_PREVIEW_RUNTIME_CONFIG) {
  throw new Error("clean setup smoke intentionally verifies discovery before optional preview configuration");
}

const transport = new StdioClientTransport({
  command: process.execPath,
  args: [serverPath],
  env: {
    PATH: process.env.PATH ?? "",
    HOME: process.env.HOME ?? "",
    NOON_REPO: repoRoot,
    NOON_PYTHON: python,
  },
  stderr: "pipe",
});
const client = new Client({ name: "noon-clean-setup-smoke", version: "0.1.0" });
try {
  await client.connect(transport);
  const tools = await client.listTools();
  assert.deepEqual(tools.tools.map((tool) => tool.name).sort(), ["noon_capabilities", "noon_reference"]);

  const capabilities = await client.callTool({
    name: "noon_capabilities",
    arguments: { symbols: ["Circle", "Transform"] },
  });
  assert.notEqual(capabilities.isError, true);
  assert.equal(capabilities.structuredContent.kind, "noon-agent-capabilities");
  assert.equal(capabilities.structuredContent.scope, "source-inventory");
  assert.equal(capabilities.structuredContent.qualification.behavioral_tests_run, false);
  assert.match(capabilities.structuredContent.provenance.input_sha256["scripts/noon-capabilities.py"], /^[0-9a-f]{64}$/);

  const reference = await client.callTool({
    name: "noon_reference",
    arguments: { example: "parity-square-to-circle" },
  });
  assert.notEqual(reference.isError, true);
  assert.match(reference.structuredContent.source, /class SquareToCircle/);
  assert.match(reference.structuredContent.source_sha256, /^[0-9a-f]{64}$/);

  process.stdout.write(`${JSON.stringify({
    ok: true,
    packageRoot,
    capabilityRevision: capabilities.structuredContent.provenance.revision,
    referenceSha256: reference.structuredContent.source_sha256,
  })}\n`);
} finally {
  await client.close().catch(() => {});
}
