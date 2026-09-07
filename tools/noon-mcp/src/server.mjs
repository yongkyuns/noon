import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import * as z from "zod/v4";
import { createDiscovery } from "./discovery.mjs";

// Startup configuration is trusted. Tools cannot select another checkout or command.
try {
  const discovery = await createDiscovery({ repoRoot: process.env.NOON_REPO, pythonExecutable: process.env.NOON_PYTHON });
  const run = (operation) => async (input, context) => {
    try {
      const result = await operation(input, { signal: context.mcpReq.signal });
      return { content: [{ type: "text", text: JSON.stringify(result) }], structuredContent: result };
    } catch (error) {
      return { isError: true, content: [{ type: "text", text: String(error.message ?? error).slice(0, 1200) }] };
    }
  };
  const handle = serveStdio(() => {
    const server = new McpServer({ name: "noon-discovery", version: "0.1.0" });
    const annotations = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false };
    server.registerTool("noon_capabilities", {
      description: "Read the configured Noon checkout's versioned support inventory. Exported APIs and declared evidence are not runtime qualification. Does not run a scene.",
      inputSchema: z.strictObject({
        symbols: z.array(z.string().regex(/^[A-Za-z_][A-Za-z0-9_]{0,127}$/)).max(32).optional(),
        examples: z.array(z.string().regex(/^[a-z0-9][a-z0-9-]{0,127}$/)).max(32).optional(),
      }), annotations,
    }, run(discovery.capabilities));
    server.registerTool("noon_reference", {
      description: "Read one ready Noon example by inventory ID, verifying its source hash. Returns source and declared restrictions/provenance, not rendered output.",
      inputSchema: z.strictObject({ example: z.string().regex(/^[a-z0-9][a-z0-9-]{0,127}$/) }), annotations,
    }, run(discovery.reference));
    return server;
  });
  let closing = false;
  const close = () => {
    if (closing) return;
    closing = true;
    discovery.close();
    void handle.close().catch((error) => { console.error(String(error.message ?? error)); process.exitCode = 1; });
  };
  process.once("SIGINT", close);
  process.once("SIGTERM", close);
  process.stdin.once("end", close);
} catch (error) {
  console.error(`Noon discovery startup failed: ${String(error.message ?? error).slice(0, 1200)}`);
  process.exitCode = 1;
}
