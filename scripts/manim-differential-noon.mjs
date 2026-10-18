#!/usr/bin/env node
// Execute the existing semantic probes through the real Rust/WASM Python facade.
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { serveRepository } from "./browser-test-server.mjs";
import { PYTHON_COMPAT_MODULES } from "../web/python-compat-modules.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.argv[2] ?? "browser-smoke-artifacts/manim-differential/noon.json");
const worker = await readFile(path.join(root, "web/python-worker.source.js"), "utf8");
const pyodideUrl = worker.match(/import \{ loadPyodide \} from "([^"]+)";/)?.[1];
assert.ok(pyodideUrl, "use the product worker's pinned Pyodide");
const modules = await Promise.all(PYTHON_COMPAT_MODULES.map(async ({ sourcePath, runtimePath }) => ({
  runtimePath, source: await readFile(path.join(root, "web", sourcePath), "utf8"),
})));
const probes = await readFile(path.join(root, "scripts/manim-differential.py"), "utf8");
const server = await serveRepository(root, Number(process.env.NOON_MANIM_DIFFERENTIAL_PORT ?? 8799));
let browser;
try {
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  page.setDefaultTimeout(180_000);
  page.on("console", message => console.log(`[browser] ${message.text()}`));
  await page.goto(`${server.baseUrl}/web/authoring-errors-smoke.html`);
  const product = await page.evaluate(async ({ modules, probes, pyodideUrl }) => {
    const wasm = await import("/web/pkg/noon_web.js");
    await wasm.default();
    const { resolveAnimationOptionsPlain } = await import("/web/animation-options.js");
    globalThis.noonResolveAnimationOptions = (...args) => resolveAnimationOptionsPlain(wasm.resolveAnimationOptions, ...args);
    globalThis.noonResolveCreateAnimationOptions = (...args) => resolveAnimationOptionsPlain(wasm.resolveCreateAnimationOptions, ...args);
    const { loadPyodide } = await import(pyodideUrl);
    const pyodide = await loadPyodide();
    const store = new wasm.WasmAuthoringStore();
    // Only supply the same typed host entrypoints as the production worker.
    // Geometry, identities, family traversal and all observations remain Rust-owned.
    globalThis.noonCreateCanonicalAuthoringSceneContext = () => store.createSceneContext();
    globalThis.noonAuthoringGeometryOptions = wasm.WasmManimGeometryOptions;
    globalThis.noonAuthoringVectorPath = () => new wasm.WasmAuthoringVectorPath();
    globalThis.noonCreateAuthoringGeometryHandle = options => store.createManimGeometry(options);
    globalThis.noonAuthoringCoordinateOptions = wasm.WasmCoordinateOptions;
    globalThis.noonCreateAuthoringCoordinateHandle = options => store.createCoordinates(options);
    globalThis.noonAuthoringBarChartOptions = (values, range, width, height) =>
      new wasm.WasmBarChartOptions(values, range, width, height);
    globalThis.noonPlotSamplingPlan = wasm.WasmPlotSamplingPlan;
    // Match the production host's explicit unavailable-Latex behavior. These
    // imports keep the plotting module intact; this differential never
    // exercises BarChart or its labels without the production Latex setup.
    globalThis.noonCreateAuthoringBarChart = (options, labels) => {
      options.free();
      labels.free();
      throw new Error("Call await prepare_latex() before constructing BarChart");
    };
    globalThis.noonBarChartLabels = () => {
      throw new Error("Call await prepare_latex() before constructing chart labels");
    };
    globalThis.noonTextColorBatch = () => new wasm.WasmTextColorBatch();
    globalThis.noonCreateAuthoringTextHandle = (...args) => store.createManimText(...args);
    globalThis.noonAuthoringMembershipBatch = kind => new wasm.WasmSceneMembershipBatch(kind);
    globalThis.noonCreateAuthoringFamilyHandle = (batch, zIndex) => store.createFamily(batch, zIndex);
    // Manim's curve-function APIs return NumPy arrays. Provision that optional
    // package in this comparison worker to match the pinned reference runtime.
    await pyodide.loadPackage("numpy");
    for (const { runtimePath, source } of modules) pyodide.FS.writeFile(runtimePath, source);
    pyodide.FS.writeFile("/tmp/noon_differential.py", probes);
    return JSON.parse(await pyodide.runPythonAsync(`
import sys, json
sys.path.insert(0, "/tmp")
from noon_differential import noon_observations
from _manim_compat import VMobject
value = VMobject()
value.n_points_per_cubic_curve = 2
try:
    value.set_anchors_and_handles([(0, 0)], [(1, 0)], [(1, 1)], [(2, 1)])
except Exception as error:
    custom_stride_gate = {
        "error_type": type(error).__name__,
        "error_message": str(error),
    }
else:
    custom_stride_gate = {"unexpected_success": True}
json.dumps({
    "observations": noon_observations(),
    "custom_stride_anchor_gate": custom_stride_gate,
}, allow_nan=False)
`));
  }, { modules, probes, pyodideUrl });
  assert.deepEqual(product.custom_stride_anchor_gate, {
    error_type: "NotImplementedError",
    error_message: "set_anchors_and_handles requires the supported four-point cubic stride",
  });
  const observations = product.observations;
  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(observations, null, 2)}\n`);
  console.log(`Observed ${Object.keys(observations).length} shared Rust/Python semantic fixtures`);
  console.log("Observed the custom-stride set_anchors_and_handles unsupported disposition in product Pyodide");
} finally {
  await browser?.close();
  await server.close();
}
