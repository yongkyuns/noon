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
    globalThis.noonCreateAuthoringFamilyHandle = (batch, zIndex, ...color) => color.length
      ? store.createFamilyWithColor(batch, zIndex, ...color)
      : store.createFamily(batch, zIndex);
    // Manim's curve-function APIs return NumPy arrays. Provision that optional
    // package in this comparison worker to match the pinned reference runtime.
    await pyodide.loadPackage("numpy");
    for (const { runtimePath, source } of modules) pyodide.FS.writeFile(runtimePath, source);
    pyodide.FS.writeFile("/tmp/noon_differential.py", probes);
    return JSON.parse(await pyodide.runPythonAsync(`
import sys, json
sys.path.insert(0, "/tmp")
from noon_differential import noon_observations
import noon
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
direction_points = [(0, 1), (1, 1), (1, 2), (2, 0)]
live_direction = VMobject()
live_direction.set_points(direction_points)
live_direction.n_points_per_cubic_curve = 2
live_scene.add(live_direction)
authored_direction = VMobject()
authored_direction.set_points(direction_points)
authored_direction.n_points_per_cubic_curve = 2
live_direction_negative = VMobject()
live_direction_negative.set_points(direction_points)
live_direction_negative.n_points_per_cubic_curve = -2
live_scene.add(live_direction_negative)
authored_direction_negative = VMobject()
authored_direction_negative.set_points(direction_points)
authored_direction_negative.n_points_per_cubic_curve = -2
default_stride_direction = VMobject()
default_stride_direction.set_points(direction_points)
def curve_points_array_contract(value):
    points = value.get_nth_curve_points(0)
    return {
        "type": type(points).__name__,
        "shape": list(points.shape),
        "dtype": str(points.dtype),
    }
def subpath_array_contracts(value):
    return [
        {
            "type": type(points).__name__,
            "shape": list(points.shape),
            "dtype": str(points.dtype),
        }
        for points in value.get_subpaths()
    ]
def point_array_contract(points):
    return {
        "type": type(points).__name__,
        "shape": list(points.shape),
        "dtype": str(points.dtype),
    }
def anchor_array_contract(value):
    anchors = value.get_anchors()
    groups = value.get_anchors_and_handles()
    return {
        "start": point_array_contract(value.get_start()),
        "end": point_array_contract(value.get_end()),
        "starts": point_array_contract(value.get_start_anchors()),
        "ends": point_array_contract(value.get_end_anchors()),
        "anchors_type": type(anchors).__name__,
        "anchors": [point_array_contract(point) for point in anchors],
        "groups_type": type(groups).__name__,
        "groups": [point_array_contract(group) for group in groups],
    }
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
    "nth_curve_points_array": curve_points_array_contract(live_custom_stride),
    "nth_curve_points": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_nth_curve_points(0)
    ],
    "start_anchors": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_start_anchors()
    ],
    "end_anchors": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_end_anchors()
    ],
    "anchors": [
        [float(coordinate) for coordinate in point]
        for point in live_custom_stride.get_anchors()
    ],
    "anchors_and_handles": [
        [[float(coordinate) for coordinate in point] for point in group]
        for group in live_custom_stride.get_anchors_and_handles()
    ],
    "subpaths": [
        [[float(coordinate) for coordinate in point] for point in subpath]
        for subpath in live_custom_stride.get_subpaths()
    ],
    "subpath_arrays": subpath_array_contracts(live_custom_stride),
    "anchor_arrays": anchor_array_contract(live_custom_stride),
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
piece_source = VMobject()
piece_source.set_points_as_corners([(0, 0), (2, 0), (2, 2)])
live_scene.add(piece_source)
authored_piece_source = VMobject()
authored_piece_source.set_points_as_corners([(0, 0), (2, 0), (2, 2)])
def path_pieces_contract(value):
    group = value.get_pieces(2)
    return {
        "group_type": type(group).__name__,
        "point_counts": [member.get_num_points() for member in group.submobjects],
        "starts": [[float(x) for x in member.get_start()] for member in group.submobjects],
        "ends": [[float(x) for x in member.get_end()] for member in group.submobjects],
    }
path_pieces = {
    "live": path_pieces_contract(piece_source),
    "authored": path_pieces_contract(authored_piece_source),
}
point_owner = VMobject(
    color=noon.GREEN,
    fill_opacity=0.35,
    stroke_color=noon.RED,
    stroke_opacity=0.6,
    stroke_width=4,
)
point_owner.set_points_as_corners([(0, 0), (2, 0), (2, 2)])
live_scene.add(point_owner)
styled_point = point_owner.get_point_mobject((3, -1))
center_point = point_owner.get_point_mobject()
direct_point = noon.VectorizedPoint()
direct_point.set_location((4, 5))
def point_mobject_contract(value):
    fill = value.get_fill_color()
    stroke = value.get_stroke_color()
    return {
        "type": type(value).__name__,
        "point_count": value.get_num_points(),
        "location": [float(x) for x in value.get_location()],
        "size": [float(value.width), float(value.height)],
        "paint": [
            [float(fill.red), float(fill.green), float(fill.blue)],
            [float(stroke.red), float(stroke.green), float(stroke.blue)],
            float(value.get_fill_opacity()),
            float(value.get_stroke_opacity()),
            float(value.get_stroke_width()),
        ],
    }
point_mobject = {
    "styled": point_mobject_contract(styled_point),
    "center": point_mobject_contract(center_point),
    "direct": point_mobject_contract(direct_point),
}
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
    "nth_curve_points_array": curve_points_array_contract(authored_custom_stride),
    "nth_curve_points": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_nth_curve_points(0)
    ],
    "start_anchors": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_start_anchors()
    ],
    "end_anchors": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_end_anchors()
    ],
    "anchors": [
        [float(coordinate) for coordinate in point]
        for point in authored_custom_stride.get_anchors()
    ],
    "anchors_and_handles": [
        [[float(coordinate) for coordinate in point] for point in group]
        for group in authored_custom_stride.get_anchors_and_handles()
    ],
    "subpaths": [
        [[float(coordinate) for coordinate in point] for point in subpath]
        for subpath in authored_custom_stride.get_subpaths()
    ],
    "subpath_arrays": subpath_array_contracts(authored_custom_stride),
    "anchor_arrays": anchor_array_contract(authored_custom_stride),
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
observations = noon_observations()
family_paint = observations["family_own_paint"]
assert observations["family_own_paint_live"] == family_paint
family_paint_play = observations["family_own_paint_play_live"]
def color_components(color):
    return (color.red, color.green, color.blue)
def assert_color(actual, expected):
    assert all(abs(float(left) - float(right)) <= 1e-6
               for left, right in zip(actual, color_components(expected))), (
                   actual, color_components(expected)
               )
assert_color(family_paint["plain_default"][0], noon.WHITE)
assert_color(family_paint["plain_default"][1][0], noon.RED)
assert_color(family_paint["plain_default"][1][1], noon.BLUE)
assert_color(family_paint["explicit_constructor_color"][0], noon.GREEN)
assert_color(family_paint["explicit_constructor_color"][1][0], noon.RED)
assert_color(family_paint["explicit_constructor_color"][1][1], noon.BLUE)
assert_color(family_paint["root_only_and_late_add"][0], noon.GREEN)
assert_color(family_paint["root_only_and_late_add"][1][0], noon.RED)
assert_color(family_paint["root_only_and_late_add"][1][1], noon.BLUE)
assert_color(family_paint["root_only_and_late_add"][1][2], noon.RED)
assert_color(family_paint["empty_root_only"][0], noon.YELLOW)
assert family_paint["empty_root_only"][1] == 0
assert_color(family_paint["gradient_root_and_leaves"][0], noon.WHITE)
assert_color(family_paint["gradient_root_and_leaves"][1][0], noon.RED)
assert_color(family_paint["gradient_root_and_leaves"][1][-1], noon.BLUE)
assert_color(family_paint["nested_shared_root_only"][0], noon.BLUE)
assert_color(family_paint["nested_shared_root_only"][1], noon.RED)
assert_color(family_paint["nested_shared_root_only"][2], noon.GREEN)
assert_color(family_paint["nested_shared_root_only"][3], noon.YELLOW)
assert family_paint["nested_shared_root_only"][4:] == [True, True]
assert_color(family_paint["vgroup_save_restore"][0], noon.WHITE)
assert_color(family_paint["vgroup_save_restore"][1][0], noon.RED)
assert_color(family_paint["vgroup_save_restore"][1][1], noon.BLUE)
assert_color(family_paint_play["before"], noon.GREEN)
assert_color(family_paint_play["target"], noon.YELLOW)
assert_color(family_paint_play["after"], noon.YELLOW)
assert_color(family_paint_play["children"][0], noon.RED)
assert_color(family_paint_play["children"][1], noon.BLUE)
json.dumps({
    "observations": observations,
    "custom_stride_anchor_gate": custom_stride_gate,
    "custom_stride_scaling_gate": custom_stride_scaling_gate,
    "custom_stride_live_queries": custom_stride_live_queries,
    "custom_stride_authored_queries": custom_stride_authored_queries,
    "path_pieces": path_pieces,
    "point_mobject": point_mobject,
    "custom_stride_direction_live": live_direction.get_direction(),
    "custom_stride_direction_authored": authored_direction.get_direction(),
    "custom_stride_negative_direction_live": live_direction_negative.get_direction(),
    "custom_stride_negative_direction_authored": authored_direction_negative.get_direction(),
    "default_stride_direction": default_stride_direction.get_direction(),
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
  assert.equal(product.custom_stride_direction_live, "CW");
  assert.equal(product.custom_stride_direction_authored, "CW");
  assert.equal(product.custom_stride_negative_direction_live, "CCW");
  assert.equal(product.custom_stride_negative_direction_authored, "CCW");
  assert.equal(product.default_stride_direction, "CCW");
  const expectedPathPieces = {
    group_type: "Group",
    point_counts: [8, 4],
    starts: [[0, 0], [2, 0]],
    ends: [[2, 0], [2, 2]],
  };
  assert.deepEqual(product.path_pieces.live, expectedPathPieces);
  assert.deepEqual(product.path_pieces.authored, expectedPathPieces);
  const styledPoint = product.point_mobject.styled;
  assert.deepEqual({
    type: styledPoint.type,
    point_count: styledPoint.point_count,
    location: styledPoint.location,
    size: styledPoint.size,
  }, {
    type: "VectorizedPoint",
    point_count: 1,
    location: [3, -1],
    size: [0.01, 0.01],
  });
  for (const [actual, expected] of [
    [styledPoint.paint[0], [131 / 255, 193 / 255, 103 / 255]],
    [styledPoint.paint[1], [252 / 255, 98 / 255, 85 / 255]],
  ]) {
    assert.ok(actual.every((value, index) => Math.abs(value - expected[index]) <= 1e-6));
  }
  assert.deepEqual(styledPoint.paint.slice(2), [0.35, 0.6, 4]);
  assert.deepEqual({
    type: product.point_mobject.center.type,
    point_count: product.point_mobject.center.point_count,
    location: product.point_mobject.center.location,
    size: product.point_mobject.center.size,
  }, {
    type: "VectorizedPoint",
    point_count: 1,
    location: [1, 1],
    size: [0.01, 0.01],
  });
  assert.deepEqual(product.point_mobject.direct, {
    type: "VectorizedPoint",
    point_count: 1,
    location: [4, 5],
    size: [0.01, 0.01],
    paint: [[0, 0, 0], [0, 0, 0], 0, 1, 0],
  });
  const {
    proportion_from_point,
    nth_curve_points_array: liveCurvePointsArray,
    subpath_arrays: liveSubpathArrays,
    anchor_arrays: liveAnchorArrays,
    ...liveQueryValues
  } = product.custom_stride_live_queries;
  const authoredCurvePointsArray =
    product.custom_stride_authored_queries.nth_curve_points_array;
  const authoredSubpathArrays =
    product.custom_stride_authored_queries.subpath_arrays;
  const authoredAnchorArrays =
    product.custom_stride_authored_queries.anchor_arrays;
  assert.deepEqual(liveCurvePointsArray, {
    type: "ndarray",
    shape: [2, 2],
    dtype: "float64",
  });
  assert.deepEqual(authoredCurvePointsArray, liveCurvePointsArray);
  const expectedSubpathArrays = [
    { type: "ndarray", shape: [2, 2], dtype: "float64" },
    { type: "ndarray", shape: [2, 2], dtype: "float64" },
  ];
  assert.deepEqual(liveSubpathArrays, expectedSubpathArrays);
  assert.deepEqual(authoredSubpathArrays, liveSubpathArrays);
  const expectedAnchorArrays = {
    start: { type: "ndarray", shape: [2], dtype: "float64" },
    end: { type: "ndarray", shape: [2], dtype: "float64" },
    starts: { type: "ndarray", shape: [2, 2], dtype: "float64" },
    ends: { type: "ndarray", shape: [2, 2], dtype: "float64" },
    anchors_type: "list",
    anchors: Array.from({ length: 4 }, () => ({
      type: "ndarray",
      shape: [2],
      dtype: "float64",
    })),
    groups_type: "list",
    groups: Array.from({ length: 2 }, () => ({
      type: "ndarray",
      shape: [2, 2],
      dtype: "float64",
    })),
  };
  assert.deepEqual(liveAnchorArrays, expectedAnchorArrays);
  assert.deepEqual(authoredAnchorArrays, liveAnchorArrays);
  assert.deepEqual(liveQueryValues, {
    x_max: 30,
    boundary_point: [30, 0],
    boundary_points: [[0, 0], [30, 0], [1, 0], [2, 0]],
    critical_point: [30, 0],
    center: [15, 0],
    num_curves: 2,
    nth_curve_points: [[0, 0], [30, 0]],
    start_anchors: [[0, 0], [1, 0]],
    end_anchors: [[30, 0], [2, 0]],
    anchors: [[0, 0], [30, 0], [1, 0], [2, 0]],
    anchors_and_handles: [[[0, 0], [1, 0]], [[30, 0], [2, 0]]],
    subpaths: [[[0, 0], [30, 0]], [[1, 0], [2, 0]]],
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
      nth_curve_points_array: product.custom_stride_live_queries.nth_curve_points_array,
      nth_curve_points: product.custom_stride_live_queries.nth_curve_points,
      start_anchors: product.custom_stride_live_queries.start_anchors,
      end_anchors: product.custom_stride_live_queries.end_anchors,
      anchors: product.custom_stride_live_queries.anchors,
      anchors_and_handles: product.custom_stride_live_queries.anchors_and_handles,
      subpaths: product.custom_stride_live_queries.subpaths,
      subpath_arrays: product.custom_stride_live_queries.subpath_arrays,
      anchor_arrays: product.custom_stride_live_queries.anchor_arrays,
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
