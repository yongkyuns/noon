// Optional real-engine qualification; downloads only integrity-pinned assets.
// This checks the host boundary. Retained-text and frontend parity have their
// own tests and must not be inferred from successful TeX compilation alone.
import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import playwright from "playwright";
import { serveRepository } from "./browser-test-server.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const server = await serveRepository(root, 0);
try {
  for (const name of ["chromium", "firefox", "webkit"]) {
    const browser = await playwright[name].launch({ headless: true });
    try {
      const page = await browser.newPage();
      await page.goto(`${server.baseUrl}/web/latex/backend.js`);
      const report = await page.evaluate(async () => {
        const { prepareLatexBackend } = await import("/web/latex/backend.js");
        const started = performance.now();
        const backend = await prepareLatexBackend();
        const prepared = performance.now();
        const document = String.raw`\usepackage{amsmath}
\begin{document}
\setbox0=\vbox{\hsize=15cm\begin{align*}x^2+\frac{1}{2}\end{align*}}
\shipout\box0
\end{document}`;
        const first = backend.compile(document).dvi;
        const compiled = performance.now();
        const second = backend.compile(document).dvi;
        const same = (left, right) => left.length === right.length && left.every((byte, i) => byte === right[i]);
        if (!same(first, second)) throw new Error("Repeated compilation changed DVI bytes");
        let failed = false;
        try { backend.compile(document.replace("x^2", "\\NoonUnknownCommand")); }
        catch { failed = true; }
        if (!failed) throw new Error("Invalid LaTeX did not fail");
        if (!same(first, backend.compile(document).dvi)) throw new Error("Failed compile contaminated engine state");
        return {
          dviBytes: first.length, preamble: [...first.slice(0, 2)],
          snapshotBytes: backend.snapshotBytes, prepareMs: prepared - started,
          compileMs: compiled - prepared, fontBytes: backend.font("cmr10").ttf.length,
          engineLicense: backend.licenses.engine.length, fontLicense: backend.licenses.fonts.length,
        };
      });
      assert.deepEqual(report.preamble, [247, 2]);
      assert.ok(report.dviBytes > 100 && report.dviBytes < 4096);
      assert.ok(report.snapshotBytes > 0 && report.snapshotBytes < 16 * 1024 * 1024);
      assert.ok(report.fontBytes > 0 && report.engineLicense > 0 && report.fontLicense > 0);
      console.log(JSON.stringify({ browser: name, ...report }));
    } finally { await browser.close(); }
  }
} finally { await server.close(); }
