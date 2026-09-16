import { createReadStream } from "node:fs";
import { stat } from "node:fs/promises";
import { createServer } from "node:http";
import path from "node:path";

// Test hosts share one bounded repository file server. A bind collision rejects
// before any browser opens instead of silently using another task's server.
export async function serveRepository(repoRoot, port, { crossOriginIsolated = false } = {}) {
  const baseUrl = `http://127.0.0.1:${port}`;
  const contentTypes = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
    ".wasm": "application/wasm", ".json": "application/json", ".py": "text/x-python" };
  const server = createServer(async (request, response) => {
    try {
      const relative = decodeURIComponent(new URL(request.url, baseUrl).pathname).replace(/^\/+/, "");
      const resolved = path.resolve(repoRoot, relative);
      if (!resolved.startsWith(`${repoRoot}${path.sep}`)) { response.writeHead(403).end(); return; }
      if (!(await stat(resolved)).isFile()) { response.writeHead(404).end(); return; }
      if (crossOriginIsolated) {
        response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
        response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
        response.setHeader("Cross-Origin-Resource-Policy", "same-origin");
      }
      response.setHeader("Content-Type", contentTypes[path.extname(resolved)] ?? "application/octet-stream");
      createReadStream(resolved).on("error", () => response.destroy()).pipe(response);
    } catch (error) {
      response.writeHead(error.code === "ENOENT" ? 404 : 500).end(String(error));
    }
  });

  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", resolve);
  });
  return { baseUrl, async close() {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  } };
}
