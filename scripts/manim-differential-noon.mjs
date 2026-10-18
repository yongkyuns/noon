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
import numpy as np
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
scaling = VMobject()
scaling.set_points([(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
scaling.n_points_per_cubic_curve = 5
try:
    scaling.scale_handle_to_anchor_distances(0.5)
except Exception as error:
    custom_stride_scaling_gate = {
        "error_type": type(error).__name__,
        "error_message": str(error),
    }
else:
    custom_stride_scaling_gate = {"unexpected_success": True}
from noon import Scene
live_scene = Scene()
live_custom_stride = VMobject()
live_custom_stride.set_points([
    (0, 0), (30, 0), (1, 0), (2, 0),
])
live_custom_stride.n_points_per_cubic_curve = 2
live_scene.add(live_custom_stride)
custom_stride_live_queries = {
    "x_max": float(live_custom_stride.get_extremum_along_dim(dim=0, key=1)),
    "boundary_point": [
        float(value)
        for value in live_custom_stride.get_boundary_point((1, 0, 0))
    ],
    "boundary_points": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_points_defining_boundary()
    ],
    "critical_point": [
        float(value)
        for value in live_custom_stride.get_critical_point((1, 0, 0))
    ],
    "center": [float(value) for value in live_custom_stride.get_center()],
    "num_curves": int(live_custom_stride.get_num_curves()),
    "nth_curve_points": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_nth_curve_points(0)
    ],
    "curve_function_midpoints": [
        np.asarray(function(0.5), dtype=float).tolist()
        for function in live_custom_stride.get_curve_functions()
    ],
    "curve_function_lengths": [
        float(length)
        for _, length in live_custom_stride.get_curve_functions_with_lengths(
            sample_points=2
        )
    ],
    "nth_curve_function_midpoint": np.asarray(
        live_custom_stride.get_nth_curve_function(1)(0.5), dtype=float
    ).tolist(),
    "nth_curve_function_length": float(
        live_custom_stride.get_nth_curve_function_with_length(
            0, sample_points=2
        )[1]
    ),
    "arc_length": float(live_custom_stride.get_arc_length()),
    "point_from_proportion": [
        round(float(value), 7)
        for value in live_custom_stride.point_from_proportion(0.5)
    ],
    "proportion_from_point": round(
        float(live_custom_stride.proportion_from_point((15.5, 0, 0))), 7
    ),
}
authored_custom_stride = VMobject()
authored_custom_stride.set_points([
    (0, 0), (30, 0), (1, 0), (2, 0),
])
authored_custom_stride.n_points_per_cubic_curve = 2
custom_stride_authored_queries = {
    "boundary_points": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_points_defining_boundary()
    ],
    "critical_point": [
        float(value)
        for value in authored_custom_stride.get_critical_point((1, 0, 0))
    ],
    "center": [float(value) for value in authored_custom_stride.get_center()],
    "num_curves": int(authored_custom_stride.get_num_curves()),
    "nth_curve_points": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_nth_curve_points(0)
    ],
    "curve_function_midpoints": [
        np.asarray(function(0.5), dtype=float).tolist()
        for function in authored_custom_stride.get_curve_functions()
    ],
    "curve_function_lengths": [
        float(length)
        for _, length in authored_custom_stride.get_curve_functions_with_lengths(
            sample_points=2
        )
    ],
    "nth_curve_function_midpoint": np.asarray(
        authored_custom_stride.get_nth_curve_function(1)(0.5), dtype=float
    ).tolist(),
    "nth_curve_function_length": float(
        authored_custom_stride.get_nth_curve_function_with_length(
            0, sample_points=2
        )[1]
    ),
    "arc_length": float(authored_custom_stride.get_arc_length()),
    "point_from_proportion": [
        round(float(value), 7)
        for value in authored_custom_stride.point_from_proportion(0.5)
    ],
    "proportion_from_point": round(
        float(authored_custom_stride.proportion_from_point((15.5, 0, 0))), 7
    ),
}
batch = VMobject()
curves = np.array([
    [[0, 0, 0], [1, 0, 0], [1, 1, 0], [2, 1, 0]],
    [[2, 1, 0], [3, 1, 0], [3, 2, 0], [4, 2, 0]],
], dtype=float)
batch.add_cubic_bezier_curves(curves)
cubic_batch_extension = batch.get_all_points().tolist()
json.dumps({
    "observations": noon_observations(),
    "custom_stride_anchor_gate": custom_stride_gate,
    "custom_stride_scaling_gate": custom_stride_scaling_gate,
    "custom_stride_live_queries": custom_stride_live_queries,
    "custom_stride_authored_queries": custom_stride_authored_queries,
    "cubic_batch_extension": cubic_batch_extension,
}, allow_nan=False)
`));
  }, { modules, probes, pyodideUrl });
  assert.deepEqual(product.custom_stride_anchor_gate, {
    error_type: "NotImplementedError",
    error_message: "set_anchors_and_handles requires the supported four-point cubic stride",
  });
  assert.deepEqual(product.custom_stride_scaling_gate, {
    error_type: "NotImplementedError",
    error_message: "scale_handle_to_anchor_distances requires the supported four-point cubic stride",
  });
  const { proportion_from_point, ...liveQueryValues } = product.custom_stride_live_queries;
  assert.deepEqual(liveQueryValues, {
    x_max: 30,
    boundary_point: [30, 0],
    boundary_points: [[0, 0], [30, 0], [1, 0], [2, 0]],
    critical_point: [30, 0],
    center: [15, 0],
    num_curves: 2,
    nth_curve_points: [[0, 0], [30, 0]],
    curve_function_midpoints: [[15, 0, 0], [1.5, 0, 0]],
    curve_function_lengths: [30, 1],
    nth_curve_function_midpoint: [1.5, 0, 0],
    nth_curve_function_length: 30,
    arc_length: 31,
    point_from_proportion: [15.5, 0],
  });
  assert.ok(
    Math.abs(proportion_from_point - 0.5) <= 5e-7,
    `expected a stride-2 inverse proportion near 0.5, got ${proportion_from_point}`,
  );
  assert.deepEqual(
    {
      boundary_points: product.custom_stride_live_queries.boundary_points,
      critical_point: product.custom_stride_live_queries.critical_point,
      center: product.custom_stride_live_queries.center,
      num_curves: product.custom_stride_live_queries.num_curves,
      nth_curve_points: product.custom_stride_live_queries.nth_curve_points,
      curve_function_midpoints: product.custom_stride_live_queries.curve_function_midpoints,
      curve_function_lengths: product.custom_stride_live_queries.curve_function_lengths,
      nth_curve_function_midpoint: product.custom_stride_live_queries.nth_curve_function_midpoint,
      nth_curve_function_length: product.custom_stride_live_queries.nth_curve_function_length,
      arc_length: product.custom_stride_live_queries.arc_length,
      point_from_proportion: product.custom_stride_live_queries.point_from_proportion,
      proportion_from_point: product.custom_stride_live_queries.proportion_from_point,
    },
    product.custom_stride_authored_queries,
  );
  assert.deepEqual(product.cubic_batch_extension, [
    [0, 0], [1, 0], [1, 1], [2, 1],
    [2, 1], [3, 1], [3, 2], [4, 2],
  ]);
  const observations = product.observations;
  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(observations, null, 2)}\n`);
  console.log(`Observed ${Object.keys(observations).length} shared Rust/Python semantic fixtures`);
  console.log("Observed two custom-stride dispositions and the intentional cubic-batch extension in product Pyodide");
} finally {
  await browser?.close();
  await server.close();
}
