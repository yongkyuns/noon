import { McpServer } from "@modelcontextprotocol/server";
import { serveStdio } from "@modelcontextprotocol/server/stdio";
import * as z from "zod/v4";
import { createDiscovery } from "./discovery.mjs";
import { loadPreviewRuntimeConfig } from "./preview-isolation.mjs";
import { AgentPreviewService } from "./preview-service.mjs";
import { registerRenderingTools } from "./rendering-tools.mjs";

function message(error) {
  return String(error?.message ?? error).slice(0, 1200);
}

// Startup configuration is trusted. Tools cannot select another checkout,
// executable, Docker image, isolation profile or command.
try {
  const discovery = await createDiscovery({ repoRoot: process.env.NOON_REPO, pythonExecutable: process.env.NOON_PYTHON });
  const previewConfigPath = process.env.NOON_PREVIEW_RUNTIME_CONFIG;
  let previewService = null;
  if (previewConfigPath !== undefined) {
    if (previewConfigPath.trim() === "") throw new Error("NOON_PREVIEW_RUNTIME_CONFIG must be a non-empty absolute path");
    const isolationConfig = await loadPreviewRuntimeConfig({
      repoRoot: process.env.NOON_REPO,
      configPath: previewConfigPath,
    });
    previewService = new AgentPreviewService({ isolationConfig });
  }

  const run = (operation) => async (input, context) => {
    try {
      const result = await operation(input, { signal: context.mcpReq.signal });
      return { content: [{ type: "text", text: JSON.stringify(result) }], structuredContent: result };
    } catch (error) {
      return { isError: true, content: [{ type: "text", text: message(error) }] };
    }
  };

  const previewScopes = new Set();
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

    if (previewService !== null) {
      const scope = previewService.openScope();
      previewScopes.add(scope);
      registerRenderingTools(server, { service: previewService, scope });
    }
    return server;
  });

  let closePromise = null;
  const close = (reason = "MCP transport disconnected") => {
    if (closePromise !== null) return closePromise;
    closePromise = (async () => {
      const failures = [];
      if (previewService !== null) {
        for (const scope of [...previewScopes]) {
          try {
            await previewService.closeScope(scope, reason);
          } catch (error) {
            failures.push(error);
          } finally {
            previewScopes.delete(scope);
          }
        }
        try {
          await previewService.dispose(reason);
        } catch (error) {
          failures.push(error);
        }
      }
      try {
        discovery.close();
      } catch (error) {
        failures.push(error);
      }
      try {
        await handle.close();
      } catch (error) {
        failures.push(error);
      }
      if (failures.length > 0) throw new AggregateError(failures, "Noon MCP shutdown failed");
    })();
    closePromise.catch((error) => {
      console.error(message(error));
      process.exitCode = 1;
    });
    return closePromise;
  };

  process.once("SIGINT", () => { void close("MCP server interrupted"); });
  process.once("SIGTERM", () => { void close("MCP server terminated"); });
  process.stdin.once("end", () => { void close("MCP transport disconnected"); });
} catch (error) {
  console.error(`Noon discovery startup failed: ${message(error)}`);
  process.exitCode = 1;
}
