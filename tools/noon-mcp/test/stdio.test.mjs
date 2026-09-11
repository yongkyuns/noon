import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { Client } from "@modelcontextprotocol/client";
import { StdioClientTransport } from "@modelcontextprotocol/client/stdio";
import { createDiscovery } from "../src/discovery.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const serverPath = path.resolve(here, "../src/server.mjs");
const root = path.resolve(here, "../../..");
const python = execFileSync("python3", ["-c", "import sys; print(sys.executable)"], { encoding: "utf8" }).trim();

test("real stdio client sees only discovery tools and receives CLI-equivalent evidence", { timeout: 30_000 }, async (t) => {
  const discovery = await createDiscovery({ repoRoot: root, pythonExecutable: python });
  t.after(() => discovery.close());
  const transport = new StdioClientTransport({ command: process.execPath, args: [serverPath],
    env: { NOON_REPO: root, NOON_PYTHON: python }, stderr: "pipe" });
  const client = new Client({ name: "noon-mcp-contract-test", version: "0.1.0" });
  t.after(() => client.close());
  await client.connect(transport);
  const listed = await client.listTools();
  assert.deepEqual(listed.tools.map((tool) => tool.name).sort(), ["noon_capabilities", "noon_reference"]);
  assert.ok(listed.tools.every((tool) => tool.annotations.readOnlyHint === true));
  const args = { symbols: ["Circle", "MathTex", "Axes"] };
  const expected = await discovery.capabilities(args);
  const response = await client.callTool({ name: "noon_capabilities", arguments: args });
  assert.notEqual(response.isError, true);
  assert.deepEqual(response.structuredContent, expected);
  assert.deepEqual(JSON.parse(response.content[0].text), expected);
  const reference = await client.callTool({ name: "noon_reference", arguments: { example: "parity-square-to-circle" } });
  assert.notEqual(reference.isError, true);
  assert.match(reference.structuredContent.source, /class SquareToCircle/);
  assert.equal(reference.structuredContent.behavioral_tests_run, false);
  for (const input of [
    { name: "noon_capabilities", arguments: { symbols: ["NoSuchNoonSymbol"] } },
    { name: "noon_capabilities", arguments: { symbols: ["--help"] } },
    { name: "noon_reference", arguments: { example: "../../secret" } },
    { name: "noon_capabilities", arguments: { repoRoot: "/" } },
  ]) {
    const invalid = await client.callTool(input);
    assert.equal(invalid.isError, true, `invalid tool input unexpectedly succeeded: ${JSON.stringify(input)}`);
  }
});

test("missing configuration fails without emitting non-protocol stdout", { timeout: 5_000 }, async () => {
  const child = spawn(process.execPath, [serverPath], { env: {}, stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "", stderr = "";
  child.stdout.on("data", (data) => { stdout += data; });
  child.stderr.on("data", (data) => { stderr += data; });
  const exit = await new Promise((resolve, reject) => { child.once("error", reject); child.once("close", resolve); });
  assert.equal(exit, 1);
  assert.equal(stdout, "");
  assert.match(stderr, /Noon discovery startup failed/);
});

test("explicit invalid preview runtime fails before protocol startup and keeps stdout clean", { timeout: 5_000 }, async () => {
  const child = spawn(process.execPath, [serverPath], {
    env: {
      NOON_REPO: root,
      NOON_PYTHON: python,
      NOON_PREVIEW_RUNTIME_CONFIG: path.join(root, "does-not-exist-preview-runtime.json"),
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "", stderr = "";
  child.stdout.on("data", (data) => { stdout += data; });
  child.stderr.on("data", (data) => { stderr += data; });
  const exit = await new Promise((resolve, reject) => { child.once("error", reject); child.once("close", resolve); });
  assert.equal(exit, 1);
  assert.equal(stdout, "");
  assert.match(stderr, /Noon discovery startup failed/);
  assert.match(stderr, /preview runtime config|ENOENT/i);
});
