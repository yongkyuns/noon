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
from noon_differential import _compare, noon_observations, _plain_group_restore_extension_probe
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
    color_rgb = lambda color: None if color is None else [
        float(color.red), float(color.green), float(color.blue)
    ]
    return {
        "type": type(value).__name__,
        "point_count": value.get_num_points(),
        "location": [float(x) for x in value.get_location()],
        "size": [float(value.width), float(value.height)],
        "paint": [
            color_rgb(fill),
            color_rgb(stroke),
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
def paint_array_contract(value):
    fill_rgbas = value.get_fill_rgbas()
    stroke_rgbas = value.get_stroke_rgbas()
    color_rgb = lambda color: None if color is None else [
        float(color.red), float(color.green), float(color.blue)
    ]
    return {
        "fill_rgbas": fill_rgbas.tolist(),
        "fill_dtype": str(fill_rgbas.dtype),
        "fill_colors": [color_rgb(color) for color in value.get_fill_colors()],
        "fill_opacities": value.get_fill_opacities().tolist(),
        "stroke_rgbas": stroke_rgbas.tolist(),
        "stroke_dtype": str(stroke_rgbas.dtype),
        "stroke_colors": [color_rgb(color) for color in value.get_stroke_colors()],
        "stroke_opacities": value.get_stroke_opacities().tolist(),
    }
paint_arrays = {
    "styled": paint_array_contract(point_owner),
    "transparent_black": paint_array_contract(direct_point),
}
def style_query_contract(value):
    style = value.get_style()
    simple = value.get_style(simple=True)
    color_rgb = lambda color: None if color is None else [
        float(color.red), float(color.green), float(color.blue)
    ]
    vector = lambda value: value.tolist() if hasattr(value, "tolist") else [float(x) for x in value]
    scalar_or_list = lambda value: value.tolist() if hasattr(value, "tolist") else [float(value)]
    return {
        "keys": sorted(style),
        "types": {
            "fill_color_list": isinstance(style["fill_color"], list),
            "fill_opacity_array": getattr(style["fill_opacity"], "ndim", 0) > 0,
            "stroke_color_list": isinstance(style["stroke_color"], list),
            "stroke_opacity_array": getattr(style["stroke_opacity"], "ndim", 0) > 0,
            "background_stroke_color_list": isinstance(style["background_stroke_color"], list),
            "background_stroke_opacity_array": getattr(style["background_stroke_opacity"], "ndim", 0) > 0,
            "sheen_direction_array": getattr(style["sheen_direction"], "ndim", 0) > 0,
        },
        "stroke_opacity": float(style["stroke_opacity"]),
        "stroke_width": float(style["stroke_width"]),
        "fill_colors": [color_rgb(color) for color in style["fill_color"]],
        "fill_opacities": scalar_or_list(style["fill_opacity"]),
        "stroke_colors": [color_rgb(color) for color in style["stroke_color"]],
        "stroke_opacities": style["stroke_opacity"].tolist()
        if getattr(style["stroke_opacity"], "ndim", 0) > 0
        else float(style["stroke_opacity"]),
        "background_stroke_colors": [color_rgb(color) for color in style["background_stroke_color"]],
        "background_stroke_width": float(style["background_stroke_width"]),
        "background_stroke_opacity": style["background_stroke_opacity"].tolist()
        if getattr(style["background_stroke_opacity"], "ndim", 0) > 0
        else float(style["background_stroke_opacity"]),
        "sheen_factor": float(style["sheen_factor"]),
        "sheen_direction": vector(style["sheen_direction"]),
        "background_image": style["background_image"],
        "simple": {
            "fill_color": color_rgb(simple["fill_color"]),
            "fill_opacity": float(simple["fill_opacity"]),
            "stroke_color": color_rgb(simple["stroke_color"]),
            "stroke_opacity": float(simple["stroke_opacity"]),
            "stroke_width": float(simple["stroke_width"]),
        },
    }
style_mutator_returns_self = [
    point_owner.set_stroke(background=True) is point_owner,
    point_owner.set_background_stroke() is point_owner,
    point_owner.set_cap_style(noon.CapStyleType.AUTO) is point_owner,
    point_owner.set_sheen(0.0, (-1.0, 1.0, 0.0)) is point_owner,
    point_owner.set_sheen_direction((-1.0, 1.0, 0.0)) is point_owner,
    point_owner.rotate_sheen_direction(0.0) is point_owner,
]
def rejected_without_style_change(operation):
    before = style_query_contract(point_owner)
    try:
        operation()
    except NotImplementedError:
        return style_query_contract(point_owner) == before
    return False
style_mutator_rejections = {
    "background_stroke": rejected_without_style_change(
        lambda: point_owner.set_background_stroke(color=noon.BLUE, width=2)
    ),
    "sheen_factor": rejected_without_style_change(
        lambda: point_owner.set_sheen(0.5)
    ),
    "cap_style": rejected_without_style_change(
        lambda: point_owner.set_cap_style(noon.CapStyleType.ROUND)
    ),
}
round_trip_style = VMobject()
round_trip_style.set_style(**point_owner.get_style())
style_queries = {
    "source": style_query_contract(point_owner),
    "round_trip": style_query_contract(round_trip_style),
}
gradient_path = noon.Rectangle(width=2, height=1).shift(0.4 * noon.RIGHT + 0.2 * noon.UP)
gradient_start, gradient_end = gradient_path.get_gradient_start_and_end_points()
gradient_start_end = {
    "start": [float(component) for component in gradient_start],
    "end": [float(component) for component in gradient_end],
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
sheen_direction_mismatch = _compare(
    observations["sheen_direction"],
    observations["sheen_direction_live"],
    1e-5,
)
assert not sheen_direction_mismatch, sheen_direction_mismatch
plain_group_restore_extension = _plain_group_restore_extension_probe(noon)
expectedFamilyPointCounts = {
    "group": {
        "before": {"root": 0, "has_points": False, "has_no_points": True, "leaf": 4},
        "after": {"root": 0, "has_points": False, "has_no_points": True, "leaf": 0},
    },
    "vgroup": {
        "before": {"root": 0, "has_points": False, "has_no_points": True, "leaf": 4},
        "after": {"root": 0, "has_points": False, "has_no_points": True, "leaf": 0},
    },
}
assert observations["family_point_counts"] == expectedFamilyPointCounts
assert observations["family_point_counts_live"] == expectedFamilyPointCounts
expected_cap_style = {
    "default": "AUTO",
    "constructed": [
        {"requested": name, "observed": name}
        for name in ("AUTO", "ROUND", "BUTT", "SQUARE")
    ],
    "mutations": [
        {"requested": name, "observed": name, "returned_self": True}
        for name in ("AUTO", "ROUND", "BUTT", "SQUARE")
    ],
    "family": {
        "root": "SQUARE",
        "child": "BUTT",
        "root_returned_self": True,
        "child_returned_self": True,
    },
}
assert observations["cap_style"] == expected_cap_style
assert observations["cap_style_live"] == expected_cap_style
expected_vgroup_nonrecursive_fade = {
    "returned_self": True,
    "root": [0.0, 0.75, 0.75],
    "children": [[0.4, 0.6, 1.0], [0.2, 0.8, 1.0]],
}
assert observations["vgroup_fade_family_false"] == expected_vgroup_nonrecursive_fade
live_fade_mismatch = _compare(
    observations["vgroup_fade_family_false_live"],
    expected_vgroup_nonrecursive_fade,
    1e-6,
)
assert not live_fade_mismatch, live_fade_mismatch
assert not _compare(
    observations["cubic_bezier"], observations["cubic_bezier_live"], 1e-5
), _compare(observations["cubic_bezier"], observations["cubic_bezier_live"], 1e-5)
assert not _compare(
    observations["vgroup_root_style_copy_target"],
    observations["vgroup_root_style_copy_target_live"],
    1e-5,
), _compare(
    observations["vgroup_root_style_copy_target"],
    observations["vgroup_root_style_copy_target_live"],
    1e-5,
)
assert not _compare(
    observations["vgroup_root_style_play"],
    observations["vgroup_root_style_play_live"],
    1e-5,
), _compare(
    observations["vgroup_root_style_play"],
    observations["vgroup_root_style_play_live"],
    1e-5,
)
assert not _compare(
    observations["vgroup_root_paint_arrays"],
    observations["vgroup_root_paint_arrays_live"],
    1e-5,
), _compare(
    observations["vgroup_root_paint_arrays"],
    observations["vgroup_root_paint_arrays_live"],
    1e-5,
)
family_paint = observations["family_own_paint"]
family_paint_live_mismatch = _compare(
    observations["family_own_paint_live"], family_paint, 1e-6
)
assert not family_paint_live_mismatch, family_paint_live_mismatch
root_only_opacity = family_paint["vgroup_root_only_fill_stroke_and_opacity"][1]
assert _compare(root_only_opacity[1:4], [0.6, 0.6, 0.6], 1e-6) == []
assert not _compare(root_only_opacity[5], root_only_opacity[6], 1e-6), (
    root_only_opacity[5], root_only_opacity[6]
)
root_only_style = family_paint["vgroup_root_only_set_style"]
assert not _compare(root_only_style[5], root_only_style[2], 1e-6), root_only_style
assert abs(float(root_only_style[6]) - 6.0) <= 1e-6, root_only_style
assert abs(float(root_only_style[7]) - 0.55) <= 1e-6, root_only_style
assert not _compare(root_only_style[8], root_only_style[9], 1e-6), (
    root_only_style[8], root_only_style[9]
)
root_background_stroke = family_paint["vgroup_root_only_background_stroke_setter"]
assert not _compare(
    root_background_stroke[0],
    [float(noon.BLUE.red), float(noon.BLUE.green), float(noon.BLUE.blue)],
    1e-6,
), root_background_stroke
assert abs(float(root_background_stroke[1]) - 7.0) <= 1e-6, root_background_stroke
assert abs(float(root_background_stroke[2]) - 0.4) <= 1e-6, root_background_stroke
assert not _compare(root_background_stroke[3], root_background_stroke[4], 1e-6), (
    root_background_stroke[3], root_background_stroke[4]
)
family_paint_play = observations["family_own_paint_play_live"]
def color_components(color):
    return (color.red, color.green, color.blue)
def assert_color(actual, expected):
    assert all(abs(float(left) - float(right)) <= 1e-6
               for left, right in zip(actual, color_components(expected))), (
                   actual, color_components(expected)
               )
def rgba(color, opacity):
    return [*color_components(color), opacity]
assert plain_group_restore_extension["status"] == "restored"
assert_color(plain_group_restore_extension["root"], noon.WHITE)
assert_color(plain_group_restore_extension["children"][0], noon.RED)
assert_color(plain_group_restore_extension["children"][1], noon.BLUE)
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
copy_target_style = observations["vgroup_root_style_copy_target"]
assert_color(copy_target_style["source"]["fill_color"], noon.GREEN)
assert_color(copy_target_style["source"]["stroke_color"], noon.BLUE)
assert_color(copy_target_style["source"]["child_fill"], noon.RED)
assert_color(copy_target_style["source"]["child_stroke"], noon.RED)
assert_color(copy_target_style["copy"]["fill_color"], noon.YELLOW)
assert_color(copy_target_style["copy"]["stroke_color"], noon.BLUE)
assert_color(copy_target_style["copy"]["child_fill"], noon.RED)
assert_color(copy_target_style["target"]["fill_color"], noon.GREEN)
assert_color(copy_target_style["target"]["stroke_color"], noon.WHITE)
assert_color(copy_target_style["target"]["child_fill"], noon.RED)
for value in copy_target_style.values():
    assert abs(value["fill_opacity"] - 0.35) <= 1e-6
    assert abs(value["stroke_opacity"] - 0.45) <= 1e-6
assert abs(copy_target_style["source"]["stroke_width"] - 4) <= 1e-6
assert abs(copy_target_style["copy"]["stroke_width"] - 4) <= 1e-6
assert abs(copy_target_style["target"]["stroke_width"] - 7) <= 1e-6
root_style_play = observations["vgroup_root_style_play_live"]
assert _compare(root_style_play["after"], root_style_play["target"], 1e-5) == []
assert_color(root_style_play["source_before"]["fill_color"], noon.RED)
assert_color(root_style_play["source_before"]["stroke_color"], noon.YELLOW)
assert_color(root_style_play["target"]["fill_color"], noon.GREEN)
assert_color(root_style_play["target"]["stroke_color"], noon.BLUE)
assert abs(root_style_play["source_before"]["fill_opacity"] - 0.1) <= 1e-6
assert abs(root_style_play["source_before"]["stroke_opacity"] - 0.3) <= 1e-6
assert abs(root_style_play["target"]["fill_opacity"] - 0.7) <= 1e-6
assert abs(root_style_play["target"]["stroke_opacity"] - 0.8) <= 1e-6
assert abs(root_style_play["source_before"]["stroke_width"] - 2) <= 1e-6
assert abs(root_style_play["target"]["stroke_width"] - 6) <= 1e-6
assert_color(root_style_play["child_after"]["fill_color"], noon.RED)
assert_color(root_style_play["child_after"]["stroke_color"], noon.RED)
assert abs(root_style_play["child_after"]["fill_opacity"] - 0.25) <= 1e-6
assert abs(root_style_play["child_after"]["stroke_opacity"] - 0.5) <= 1e-6
assert abs(root_style_play["child_after"]["stroke_width"] - 2) <= 1e-6
root_paint_arrays = observations["vgroup_root_paint_arrays"]["root"]
child_paint_arrays = observations["vgroup_root_paint_arrays"]["child"]
for value in (root_paint_arrays, child_paint_arrays):
    assert value["fill_dtype"] == "float64"
    assert value["stroke_dtype"] == "float64"
assert _compare(root_paint_arrays["fill_rgbas"], [rgba(noon.GREEN, 0.35)], 1e-6) == []
assert _compare(root_paint_arrays["stroke_rgbas"], [rgba(noon.BLUE, 0.45)], 1e-6) == []
assert _compare(child_paint_arrays["fill_rgbas"], [rgba(noon.RED, 0.25)], 1e-6) == []
assert _compare(child_paint_arrays["stroke_rgbas"], [rgba(noon.RED, 0.5)], 1e-6) == []
assert root_paint_arrays["fill_opacities"] == [0.35]
assert root_paint_arrays["stroke_opacities"] == [0.45]
assert_color(family_paint["vgroup_save_restore"][0], noon.WHITE)
assert_color(family_paint["vgroup_save_restore"][1][0], noon.RED)
assert_color(family_paint["vgroup_save_restore"][1][1], noon.BLUE)
root_style_restore = family_paint["vgroup_root_style_save_restore"]
assert_color(root_style_restore[0], noon.GREEN)
assert abs(root_style_restore[1] - 0.3) <= 1e-6
assert_color(root_style_restore[2], noon.BLUE)
assert abs(root_style_restore[3] - 3) <= 1e-6
assert abs(root_style_restore[4] - 0.5) <= 1e-6
assert_color(root_style_restore[5][0], noon.RED)
assert_color(root_style_restore[5][1], noon.BLUE)
assert_color(family_paint_play["before"], noon.GREEN)
assert_color(family_paint_play["target"], noon.YELLOW)
assert_color(family_paint_play["after"], noon.YELLOW)
assert_color(family_paint_play["children"][0], noon.RED)
assert_color(family_paint_play["children"][1], noon.BLUE)
json.dumps({
    "observations": observations,
    "plain_group_restore_extension": plain_group_restore_extension,
    "custom_stride_anchor_gate": custom_stride_gate,
    "custom_stride_scaling_gate": custom_stride_scaling_gate,
    "custom_stride_live_queries": custom_stride_live_queries,
    "custom_stride_authored_queries": custom_stride_authored_queries,
    "path_pieces": path_pieces,
    "point_mobject": point_mobject,
    "paint_arrays": paint_arrays,
    "style_queries": style_queries,
    "gradient_start_end": gradient_start_end,
    "style_mutator_returns_self": style_mutator_returns_self,
    "style_mutator_rejections": style_mutator_rejections,
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
    paint: [null, [0, 0, 0], 0, 1, 0],
  });
  const styledPaint = product.paint_arrays.styled;
  assert.deepEqual(styledPaint.fill_dtype, "float64");
  assert.deepEqual(styledPaint.stroke_dtype, "float64");
  assert.deepEqual(styledPaint.fill_opacities, [0.35]);
  assert.deepEqual(styledPaint.stroke_opacities, [0.6]);
  for (const [actual, expected] of [
    [styledPaint.fill_colors[0], [131 / 255, 193 / 255, 103 / 255]],
    [styledPaint.stroke_colors[0], [252 / 255, 98 / 255, 85 / 255]],
    [styledPaint.fill_rgbas[0], [131 / 255, 193 / 255, 103 / 255, 0.35]],
    [styledPaint.stroke_rgbas[0], [252 / 255, 98 / 255, 85 / 255, 0.6]],
  ]) {
    assert.ok(actual.every((value, index) => Math.abs(value - expected[index]) <= 1e-6));
  }
  assert.deepEqual(product.paint_arrays.transparent_black, {
    fill_rgbas: [[0, 0, 0, 0]],
    fill_dtype: "float64",
    fill_colors: [null],
    fill_opacities: [0],
    stroke_rgbas: [[0, 0, 0, 1]],
    stroke_dtype: "float64",
    stroke_colors: [[0, 0, 0]],
    stroke_opacities: [1],
  });
  const expectedStyleKeys = [
    "background_image",
    "background_stroke_color",
    "background_stroke_opacity",
    "background_stroke_width",
    "fill_color",
    "fill_opacity",
    "sheen_direction",
    "sheen_factor",
    "stroke_color",
    "stroke_opacity",
    "stroke_width",
  ];
  for (const style of [product.style_queries.source, product.style_queries.round_trip]) {
    assert.deepEqual(style.keys, expectedStyleKeys);
    assert.deepEqual(style.types, {
      fill_color_list: true,
      fill_opacity_array: true,
      stroke_color_list: true,
      stroke_opacity_array: false,
      background_stroke_color_list: true,
      background_stroke_opacity_array: false,
      sheen_direction_array: true,
    });
    assert.deepEqual(style.stroke_opacity, 0.6);
    assert.deepEqual(style.stroke_width, 4);
    assert.deepEqual(style.fill_opacities, [0.35]);
    assert.deepEqual(style.stroke_opacities, 0.6);
    assert.deepEqual(style.background_stroke_colors, [[0, 0, 0]]);
    assert.deepEqual(style.background_stroke_width, 0);
    assert.deepEqual(style.background_stroke_opacity, 1);
    assert.deepEqual(style.sheen_factor, 0);
    assert.equal(style.background_image, null);
    assert.deepEqual(style.simple.fill_opacity, 0.35);
    assert.deepEqual(style.simple.stroke_opacity, 0.6);
    assert.deepEqual(style.simple.stroke_width, 4);
    for (const [actual, expected] of [
      [style.fill_colors[0], [131 / 255, 193 / 255, 103 / 255]],
      [style.stroke_colors[0], [252 / 255, 98 / 255, 85 / 255]],
      [style.simple.fill_color, [131 / 255, 193 / 255, 103 / 255]],
      [style.simple.stroke_color, [252 / 255, 98 / 255, 85 / 255]],
    ]) {
      assert.ok(actual.every((value, index) => Math.abs(value - expected[index]) <= 1e-6));
    }
  }
  assert.deepEqual(product.style_queries.source.sheen_direction, [-1, 1, 0]);
  assert.deepEqual(product.style_queries.round_trip.sheen_direction, [-1, 1, 0]);
  assert.deepEqual(product.style_mutator_returns_self, [true, true, true, true, true, true]);
  assert.deepEqual(product.style_mutator_rejections, {
    background_stroke: true,
    sheen_factor: true,
    cap_style: false,
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
  assert.equal(product.plain_group_restore_extension.status, "restored");
  const observations = product.observations;
  for (const name of ["apply_over_attr_arrays", "apply_over_attr_arrays_live"]) {
    const profile = observations[name];
    assert.equal(profile.returns_self, true, `${name}: returns the same Mobject`);
    assert.equal(profile.calls.length, 1, `${name}: maps only the receiver's points`);
    assert.equal(profile.calls[0].length, 4, `${name}: exposes the receiver's four curve points`);
    assert.ok(profile.calls[0].every(point => point.length === 3 && point[2] === 0),
      `${name}: supplies Manim-compatible planar three-column points`);
    for (let index = 0; index < 4; index += 1) {
      assert.ok(Math.abs(profile.root_points[index][0] - (profile.calls[0][index][0] + 1.5)) < 1e-6,
        `${name}: mapped root x point ${index}`);
      assert.ok(Math.abs(profile.root_points[index][1] - (profile.calls[0][index][1] - 2)) < 1e-6,
        `${name}: mapped root y point ${index}`);
    }
    assert.deepEqual(profile.child_points, profile.child_before,
      `${name}: receiver mapping leaves siblings unchanged`);
  }
  assert.deepEqual(observations.apply_over_attr_arrays.child_points,
    observations.apply_over_attr_arrays_live.child_points,
    "array mapping preserves child point arrays in live authoring");
  assert.deepEqual(observations.apply_over_attr_arrays.root_points,
    observations.apply_over_attr_arrays_live.root_points,
    "array mapping publishes the same points in authored and live modes");
  for (const name of [
    "plain_group_fade_family_false",
    "plain_group_fade_family_false_live",
  ]) {
    const profile = observations[name];
    assert.equal(profile.returned_self, true, `${name}: fade returns the same Group`);
    assert.deepEqual(profile.after, profile.before, `${name}: family=False leaves Group children unchanged`);
  }
  await mkdir(path.dirname(output), { recursive: true });
  await writeFile(output, `${JSON.stringify(observations, null, 2)}\n`);
  console.log(`Observed ${Object.keys(observations).length} shared Rust/Python semantic fixtures`);
  console.log("Observed plain Group restore as a Noon extension, two custom-stride dispositions, and the intentional cubic-batch extension in product Pyodide");
} finally {
  await browser?.close();
  await server.close();
}
