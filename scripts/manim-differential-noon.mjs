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
    globalThis.noonAuthoringImageOptions = wasm.WasmImageMobjectOptions;
    globalThis.noonCreateAuthoringImageHandle = options => store.createImage(options);
    // These dimension probes use inline RGBA8 arrays. URL I/O is qualified
    // separately through the real production worker's image-input lane.
    globalThis.noonLoadImageUrl = async () => {
      throw new Error("image URLs are outside the inline dimension profile");
    };
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
from _manim_compat import VMobject, VGroup
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
def make_alignment_pair():
    left = VMobject(color=noon.RED, fill_opacity=0.25, stroke_color=noon.GREEN,
                    stroke_opacity=0.4, stroke_width=2)
    left.set_points_as_corners([(0, 0), (3, 0)])
    right = VMobject(color=noon.BLUE, fill_opacity=0.75, stroke_color=noon.YELLOW,
                     stroke_opacity=0.8, stroke_width=5)
    right.set_points_as_corners([(0, 0), (1, 2), (2, 2), (3, 0)])
    right.insert_n_curves(2)
    return left, right
def align_path_pair(left, right):
    left.align_points(right)
    return {
        "counts": [left.get_num_points(), right.get_num_points()],
        "fills": [left.get_fill_rgbas().tolist(), right.get_fill_rgbas().tolist()],
        "strokes": [left.get_stroke_rgbas().tolist(), right.get_stroke_rgbas().tolist()],
        "background_strokes": [
            left.get_stroke_rgbas(background=True).tolist(),
            right.get_stroke_rgbas(background=True).tolist(),
        ],
        "widths": [float(left.get_stroke_width()), float(right.get_stroke_width())],
        "background_widths": [
            float(left.get_stroke_width(background=True)),
            float(right.get_stroke_width(background=True)),
        ],
    }
live_alignment_left, live_alignment_right = make_alignment_pair()
live_scene.add(live_alignment_left, live_alignment_right)
authored_alignment_left, authored_alignment_right = make_alignment_pair()
path_alignment_paint = {
    "live": align_path_pair(live_alignment_left, live_alignment_right),
    "authored": align_path_pair(authored_alignment_left, authored_alignment_right),
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
    "sheen_factor": rejected_without_style_change(
        lambda: point_owner.set_sheen(0.5)
    ),
    "cap_style": rejected_without_style_change(
        lambda: point_owner.set_cap_style(noon.CapStyleType.ROUND)
    ),
}
def rejected_3d_operation(operation):
    try:
        operation()
    except NotImplementedError:
        return True
    return False
shade_candidate = VMobject()
shade_child = VMobject()
shade_family = VGroup(shade_child)
shade_in_3d_disposition = {
    "constructor": rejected_3d_operation(lambda: VMobject(shade_in_3d=True)),
    "vgroup_constructor": rejected_3d_operation(lambda: VGroup(shade_in_3d=True)),
    "setter": rejected_3d_operation(lambda: shade_candidate.set_shade_in_3d(True)),
    "grouping": rejected_3d_operation(
        lambda: shade_family.set_shade_in_3d(False, z_index_as_group=True)
    ),
    "states_after_rejection": [
        shade_candidate.shade_in_3d,
        shade_family.shade_in_3d,
        shade_child.shade_in_3d,
    ],
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
def rgb(color):
    return [float(color.red), float(color.green), float(color.blue)]
red_rgbas, blue_rgbas, green_rgbas = rgb(noon.RED), rgb(noon.BLUE), rgb(noon.GREEN)
expected_generated_rgbas = [
    {"values": [[0.0, 0.0, 0.0, 0.25]], "shape": [1, 4], "dtype": "float64"},
    {"values": [red_rgbas + [0.5], blue_rgbas + [0.5]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [green_rgbas + [0.2], green_rgbas + [0.8]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [red_rgbas + [0.0], [1.0, 1.0, 0.0, 0.6]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [[0.1, 0.25, 0.5, 0.3], [1.0, 128.0 / 255.0, 0.0, 0.6]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [[0.1, 0.2, 0.3, 0.3], [0.9, 0.8, 0.7, 0.5]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [[1.0, 0.0, 136.0 / 255.0, 0.2],
                [17.0 / 255.0, 34.0 / 255.0, 51.0 / 255.0, 0.4]],
     "shape": [2, 4], "dtype": "float64"},
    {"values": [[18.0 / 255.0, 52.0 / 255.0, 86.0 / 255.0, 0.9]],
     "shape": [1, 4], "dtype": "float64"},
]
assert len(observations["generate_rgbas_array"]) == len(expected_generated_rgbas)
for actual, expected in zip(observations["generate_rgbas_array"],
                            expected_generated_rgbas, strict=True):
    assert actual["shape"] == expected["shape"], (actual, expected)
    assert actual["dtype"] == expected["dtype"], (actual, expected)
    assert _compare(actual["values"], expected["values"], 1e-12) == [], (actual, expected)
pathAlignmentPaint = path_alignment_paint
assert pathAlignmentPaint["live"] == pathAlignmentPaint["authored"]
assert pathAlignmentPaint["live"]["counts"][0] == pathAlignmentPaint["live"]["counts"][1]
assert pathAlignmentPaint["live"]["fills"][0] != pathAlignmentPaint["live"]["fills"][1]
assert pathAlignmentPaint["live"]["strokes"][0] != pathAlignmentPaint["live"]["strokes"][1]
assert pathAlignmentPaint["live"]["background_strokes"][0] == pathAlignmentPaint["live"]["background_strokes"][1]
assert pathAlignmentPaint["live"]["widths"] == [2, 5]
assert pathAlignmentPaint["live"]["background_widths"] == [0, 0]
interpolateColor = observations["interpolate_color"]
interpolateColorLive = observations["interpolate_color_live"]
assert interpolateColor["returned_self"] is True
assert interpolateColorLive["returned_self"] is True
assert _compare(interpolateColor, interpolateColorLive, 1e-5) == []
assert abs(interpolateColor["fill_opacity"] - 0.35) < 1e-5
assert abs(interpolateColor["stroke_opacity"] - 0.375) < 1e-5
assert abs(interpolateColor["stroke_width"] - 3.0) < 1e-5
assert _compare(interpolateColor["sheen_direction"], [-0.5, 0.5, 0.0], 1e-5) == []
expectedVGroupInterpolation = {
    "returned_self": True,
    "fill": [[0.82745098, 0.48039216, 0.46666667, 0.35]],
    "stroke": [[0.50588235, 0.67254902, 0.73333333, 0.375]],
    "background_stroke": [[0.63529412, 0.81764706, 0.55294118, 0.3]],
    "stroke_width": 3.0,
    "background_stroke_width": 3.0,
    "sheen_direction": [-0.5, 0.5, 0.0],
    "child_unchanged": True,
}
for vgroup_interpolation_key in (
    "vgroup_interpolate_color",
    "vgroup_interpolate_color_live",
    "vgroup_interpolate",
    "vgroup_interpolate_live",
):
    vgroup_interpolation_differences = _compare(
        observations[vgroup_interpolation_key], expectedVGroupInterpolation, 1e-5
    )
    assert not vgroup_interpolation_differences, (
        vgroup_interpolation_key,
        observations[vgroup_interpolation_key],
        vgroup_interpolation_differences,
    )
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
leaf_background_stroke = family_paint["leaf_background_stroke"]
assert leaf_background_stroke["returned_self"] is True, leaf_background_stroke
assert not _compare(
    leaf_background_stroke["color"],
    [float(noon.BLUE.red), float(noon.BLUE.green), float(noon.BLUE.blue)],
    1e-6,
), (
    leaf_background_stroke
)
assert abs(float(leaf_background_stroke["width"]) - 3.0) <= 1e-6, leaf_background_stroke
assert abs(float(leaf_background_stroke["opacity"]) - 0.4) <= 1e-6, leaf_background_stroke
assert abs(float(leaf_background_stroke["set_opacity"]) - 0.6) <= 1e-6, leaf_background_stroke
assert abs(float(leaf_background_stroke["fade"]) - 0.3) <= 1e-6, leaf_background_stroke
recursive_background_stroke = family_paint["recursive_background_stroke"]
assert recursive_background_stroke[0] is True, recursive_background_stroke
assert len(recursive_background_stroke[1]) == 4, recursive_background_stroke
for color, width, opacity in recursive_background_stroke[1]:
    assert not _compare(
        color,
        [float(noon.GREEN.red), float(noon.GREEN.green), float(noon.GREEN.blue)],
        1e-6,
    ), recursive_background_stroke
    assert abs(float(width) - 2.0) <= 1e-6, recursive_background_stroke
    assert abs(float(opacity) - 0.35) <= 1e-6, recursive_background_stroke
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
    "path_alignment_paint": path_alignment_paint,
    "point_mobject": point_mobject,
    "paint_arrays": paint_arrays,
    "style_queries": style_queries,
    "gradient_start_end": gradient_start_end,
    "style_mutator_returns_self": style_mutator_returns_self,
    "style_mutator_rejections": style_mutator_rejections,
    "shade_in_3d_disposition": shade_in_3d_disposition,
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
    sheen_factor: true,
    cap_style: false,
  });
  assert.deepEqual(product.shade_in_3d_disposition, {
    constructor: true,
    vgroup_constructor: true,
    setter: true,
    grouping: true,
    states_after_rejection: [false, false, false],
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
  const inverseRangeBoundary = await page.evaluate(async () => {
    const wasm = await import('/web/pkg/noon_web.js');
    const store = new wasm.WasmAuthoringStore();
    const line = store.createManimGeometry(wasm.WasmManimGeometryOptions.line(0, 0, 1, 0));
    const query = line.pathQuery();
    try {
      let rejected = null;
      try {
        query.proportionFromPointForRanges(new Uint32Array([0]), 1, 0, 0);
      } catch (error) {
        rejected = { category: error.category, code: error.code };
      }
      return { rejected, recovered: query.proportionFromPointForRanges(new Uint32Array([0, 4]), 1, 0, 0) };
    } finally {
      query.free();
      line.free();
      store.free();
    }
  });
  assert.deepEqual(inverseRangeBoundary, {
    rejected: { category: 'invalid_input', code: 'path_query.invalid_curve_ranges' }, recovered: 1,
  });
  console.log('[PASS] typed inverse range input rejection and snapshot reuse');
  const observations = product.observations;
  const curveQueries = observations.curve_function_queries;
  for (const name of ["functions", "functions_with_lengths"]) {
    const escaped = curveQueries.escaped_after_generator_error[name];
    assert.equal(escaped.error_message, "stop querying");
    assert.deepEqual(escaped.curve, curveQueries.functions[0],
      `${name}: an escaped curve remains usable after its generator fails`);
  }
  const assertStyleNear = (actual, expected, message) => {
    for (const key of ["fill_color", "stroke_color", "background_stroke_color"]) {
      if (!(key in expected)) continue;
      assert.equal(actual[key].length, expected[key].length,
        `${message}: ${key} dimensions`);
      for (let index = 0; index < expected[key].length; index += 1) {
        assert.ok(Math.abs(actual[key][index] - expected[key][index]) <= 1e-5,
          `${message}: ${key}[${index}] (${actual[key][index]} vs ${expected[key][index]})`);
      }
    }
    for (const key of ["fill_opacity", "stroke_width", "stroke_opacity", "background_stroke_width", "background_stroke_opacity"]) {
      if (!(key in expected)) continue;
      assert.ok(Math.abs(actual[key] - expected[key]) <= 1e-5,
        `${message}: ${key} (${actual[key]} vs ${expected[key]})`);
    }
  };
  for (const name of ["recursive_family_paint", "recursive_family_paint_live"]) {
    const profile = observations[name];
    assert.deepEqual(profile.returns, [true, true, true], `${name}: preserves receiver identity`);
    assertStyleNear(profile.unrelated_after, profile.unrelated_before,
      `${name}: paint edits stay within the selected family`);
    for (const member of ["roots", "leaves"]) {
      for (let index = 0; index < 2; index += 1) {
        assertStyleNear(profile.after_fill[member][index], {
          fill_color: profile.expected_colors.green, fill_opacity: 0.4,
        }, `${name}: ${member} ${index} fill`);
        assertStyleNear(profile.after_stroke[member][index], {
          ...profile.after_fill[member][index],
          stroke_color: profile.expected_colors.blue, stroke_width: 8, stroke_opacity: 0.6,
        }, `${name}: ${member} ${index} stroke preserves other channels`);
        assertStyleNear(profile.after_opacity[member][index], {
          ...profile.after_stroke[member][index],
          fill_opacity: 0.25, stroke_opacity: 0.25, background_stroke_opacity: 0.25,
        }, `${name}: ${member} ${index} opacity preserves paint and widths`);
      }
    }
  }
  for (const phase of ["after_fill", "after_stroke", "after_opacity"]) {
    for (const member of ["roots", "leaves"]) {
      for (let index = 0; index < 2; index += 1) {
        assertStyleNear(observations.recursive_family_paint_live[phase][member][index],
          observations.recursive_family_paint[phase][member][index],
          `recursive paint ${phase} ${member} ${index}: authored/live agreement`);
      }
    }
  }
  const assertFadedStyle = (actual, before, factor, message) => assertStyleNear(actual, {
    ...before,
    fill_opacity: before.fill_opacity * factor,
    stroke_opacity: before.stroke_opacity * factor,
    background_stroke_opacity: before.background_stroke_opacity * factor,
  }, message);
  for (const name of ["recursive_family_fade", "recursive_family_fade_live"]) {
    const profile = observations[name];
    assert.deepEqual(profile.returns, [true, true, true]);
    for (const [member, factors] of [["roots", [0.5, 0.5, 0.25]], ["leaves", [0.25, 0.5]]]) {
      for (let index = 0; index < factors.length; index += 1) {
        const factor = factors[index];
        assertFadedStyle(profile.after_first[member][index], profile.before[member][index], factor,
          `${name}: first ${member} ${index} fade counts alias paths`);
        assertStyleNear(profile.after_zero[member][index], profile.after_first[member][index],
          `${name}: zero darkness preserves ${member} ${index}`);
        assertFadedStyle(profile.after_second[member][index], profile.before[member][index], factor ** 2,
          `${name}: repeated ${member} ${index} fade composes`);
      }
    }
    assert.equal(profile.empty_returned, true);
    assertFadedStyle(profile.empty, profile.empty_before, 0.5,
      `${name}: empty family root retains observable fade`);
    assert.equal(profile.plain.returned_self, true);
    assert.deepEqual(profile.plain.color_after, profile.plain.color_before,
      `${name}: plain Group own color stays unchanged`);
    assertFadedStyle(profile.plain.after, profile.plain.before, 0.25,
      `${name}: plain Group recursion reaches aliased VGroup roots`);
    assertStyleNear(profile.unrelated_after, profile.unrelated_before,
      `${name}: unrelated paint stays unchanged`);
  }
  for (const phase of ["after_first", "after_zero", "after_second"]) {
    for (const member of ["roots", "leaves"]) {
      const direct = observations.recursive_family_fade[phase][member];
      const live = observations.recursive_family_fade_live[phase][member];
      direct.forEach((style, index) => assertStyleNear(live[index], style,
        `recursive fade ${phase} ${member} ${index}: authored/live agreement`));
    }
  }
  for (const name of ["vmobject_init_colors", "vmobject_init_colors_live"]) {
    const profile = observations[name];
    assert.equal(profile.propagated.returned_self, true,
      `${name}: propagating init_colors returns the family`);
    assertStyleNear(profile.propagated.after.root, profile.propagated.before.root,
      `${name}: propagation preserves the root's own style`);
    for (const child of profile.propagated.after.children) {
      assertStyleNear(child, profile.propagated.before.root,
        `${name}: propagation applies root fill and stroke to every child`);
    }
    assert.equal(profile.family_false.returned_self, true,
      `${name}: family=False init_colors returns the family`);
    assertStyleNear(profile.family_false.after.root, profile.family_false.before.root,
      `${name}: family=False retains the root style`);
    for (let index = 0; index < profile.family_false.after.children.length; index += 1) {
      assertStyleNear(profile.family_false.after.children[index],
        profile.family_false.before.children[index],
        `${name}: family=False retains child ${index} style`);
    }
  }
  for (const mode of ["propagated", "family_false"]) {
    for (const member of ["root", "children"]) {
      const authored = observations.vmobject_init_colors[mode].after[member];
      const live = observations.vmobject_init_colors_live[mode].after[member];
      const authoredMembers = Array.isArray(authored) ? authored : [authored];
      const liveMembers = Array.isArray(live) ? live : [live];
      for (let index = 0; index < authoredMembers.length; index += 1) {
        assertStyleNear(liveMembers[index], authoredMembers[index],
          `init_colors ${mode} ${member} ${index}: authored/live result`);
      }
    }
  }
  for (const name of ["update_rgbas_array", "update_rgbas_array_live"]) {
    const profile = observations[name];
    assert.deepEqual(profile.background.returns, [true, true, true]);
    assert.ok(profile.background.color.every((channel, index) =>
      Math.abs(channel - profile.expected.green[index]) <= 1e-5));
    assert.ok(Math.abs(profile.background.width - 4) <= 1e-5);
    assert.ok(Math.abs(profile.background.opacity - 0.45) <= 1e-5);
    assert.deepEqual(profile.returns, [true, true, true, true, true],
      `${name}: each scalar update returns the same VMobject`);
    assertStyleNear(profile.after_fill_color, {
      ...profile.before,
      fill_color: profile.expected.yellow,
    }, `${name}: color-only fill update preserves opacity`);
    assertStyleNear(profile.after_stroke_color, {
      ...profile.after_fill_color,
      fill_opacity: 0.75,
      stroke_color: [1, 128 / 255, 0],
    }, `${name}: color-only stroke update preserves opacity`);
    assertStyleNear(profile.after, {
      ...profile.after_stroke_color,
      fill_opacity: 0.75,
      stroke_opacity: 0.65,
    }, `${name}: explicit opacity updates replace only alpha`);
    assert.equal(profile.family.returned_self, true,
      `${name}: VGroup update returns the same family`);
    assertStyleNear(profile.family.after.root, {
      ...profile.family.before.root,
      fill_color: profile.expected.green,
      fill_opacity: 0.35,
    }, `${name}: VGroup update changes only root paint`);
    assertStyleNear(profile.family.after.child, profile.family.before.child,
      `${name}: VGroup update leaves child paint unchanged`);
  }
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
  const assertPointNear = (actual, expected, message) => {
    assert.equal(actual.length, expected.length, `${message}: dimensions`);
    for (let index = 0; index < expected.length; index += 1) {
      assert.ok(Math.abs(actual[index] - expected[index]) < 1e-5,
        `${message}: component ${index}`);
    }
  };
  for (const name of ["align_to_top", "align_to_top_live", "align_to_group_top", "align_to_group_top_live"]) {
    const profile = observations[name];
    assert.equal(profile.returns_self, true, `${name}: returns the same Mobject`);
    assertPointNear(profile.object_top.slice(1), profile.target_top.slice(1),
      `${name}: aligns the selected top edge`);
    assertPointNear([profile.object.center[0]], [profile.before.center[0]],
      `${name}: preserves the unselected x coordinate`);
  }
  for (const name of ["align_to_point_right", "align_to_point_right_live"]) {
    const profile = observations[name];
    assert.equal(profile.returns_self, true, `${name}: returns the same Mobject`);
    assertPointNear(profile.right, [profile.point[0], profile.before.center[1]],
      `${name}: aligns x to the point and preserves y`);
  }
  for (const name of ["next_to_live", "next_to_point_live"]) {
    assert.equal(observations[name].returns_self, true,
      `${name}: returns the same Mobject after live placement`);
  }
  const nextToLive = observations.next_to_live;
  assertPointNear(
    [nextToLive.receiver.center[0] - nextToLive.receiver.width / 2],
    [nextToLive.target.center[0] + nextToLive.target.width / 2 + 0.25],
    "next_to_live: places the receiver beyond the target's right bound",
  );
  assertPointNear(
    [nextToLive.receiver.center[1] + nextToLive.receiver.height / 2],
    [nextToLive.target.center[1] + nextToLive.target.height / 2],
    "next_to_live: aligns the selected top edges",
  );
  assertPointNear(
    [observations.next_to_point_live.receiver.center[0]], [1.25],
    "next_to_point_live: preserves point x placement",
  );
  assertPointNear(
    [observations.next_to_point_live.receiver.center[1] -
      observations.next_to_point_live.receiver.height / 2],
    [0.95],
    "next_to_point_live: applies the point buffer to the bottom edge",
  );
  for (const [directName, liveName] of [
    ["to_edge", "to_edge_live"],
    ["to_corner", "to_corner_live"],
    ["align_on_border", "align_on_border_live"],
  ]) {
    const direct = observations[directName];
    const live = observations[liveName];
    for (const [name, profile] of [[directName, direct], [liveName, live]]) {
      assert.equal(profile.returns_self, true, `${name}: returns the same Mobject`);
    }
    assertPointNear(live.object.center, direct.object.center,
      `${liveName}: frame-aligned center matches detached authoring`);
    assertPointNear([live.object.width, live.object.height],
      [direct.object.width, direct.object.height],
      `${liveName}: alignment preserves geometry`);
  }
  for (const name of ["point_function_transforms", "point_function_transforms_live"]) {
    const profile = observations[name];
    assert.equal(profile.pointwise.returns_self, true, `${name}: apply_function identity`);
    assert.equal(profile.pointwise.calls.length, 8, `${name}: shared aliases map each unique path once`);
    assertPointNear(profile.pointwise.calls[0].slice(0, 2), [3, 1], `${name}: late-alias path order`);
    assertPointNear(profile.pointwise.calls[4].slice(0, 2), [1, -1], `${name}: next unique path order`);
    assertPointNear(profile.pointwise.second[0], [5, 0], `${name}: pointwise second start`);
    assertPointNear(profile.pointwise.second[3], [6, 1], `${name}: pointwise second end`);
    assertPointNear(profile.pointwise.first[0], [3, -2], `${name}: pointwise first start`);
    assertPointNear(profile.pointwise.first[3], [4, -1], `${name}: pointwise first end`);
    assert.equal(profile.multi_mapping.returns_self, true, `${name}: multi-map identity`);
    assertPointNear(profile.multi_mapping.first[0], [4, -1], `${name}: pivoted first start`);
    assertPointNear(profile.multi_mapping.second[3], [-1, 1], `${name}: pivoted second end`);
    assert.equal(profile.complex_mapping.returns_self, true, `${name}: complex-map identity`);
    assertPointNear(profile.complex_mapping.points[0], [0, 1], `${name}: complex first point`);
    assertPointNear(profile.complex_mapping.points[3], [-1, 2], `${name}: complex last point`);
  }
  for (const name of ["apply_matrix", "apply_matrix_live"]) {
    const profile = observations[name];
    assert.equal(profile.family.returns_self, true, `${name}: family matrix identity`);
    assertPointNear(profile.family.first[0], [3, 1], `${name}: family first start`);
    assertPointNear(profile.family.second[3], [0, 4], `${name}: family second end`);
    assert.equal(profile.rectangular.returns_self, true, `${name}: rectangular matrix identity`);
    assertPointNear(profile.rectangular.points[0], [2, -1], `${name}: rectangular first point`);
    assertPointNear(profile.rectangular.points[3], [4, 1], `${name}: rectangular last point`);
  }
  const animationOverrideRegistry = observations.animation_override_registry;
  assert.equal(animationOverrideRegistry.registered, true,
    "animation_override_registry: direct registration is discoverable");
  assert.equal(animationOverrideRegistry.child_isolated, true,
    "animation_override_registry: direct registrations stay on their class");
  assert.deepEqual(animationOverrideRegistry.duplicate_error, [
    "MultiAnimationOverrideException",
    "The animation Animation for Base is overridden by more than one method: "
      + "_animation_override_registry.<locals>.first_override and "
      + "_animation_override_registry.<locals>.second_override.",
  ], "animation_override_registry: duplicate registrations match Manim diagnostics");
  assert.equal(animationOverrideRegistry.decorator_preserves_function, true,
    "animation_override_registry: override_animation returns the original method");
  assert.equal(animationOverrideRegistry.intrinsic_discovered, true,
    "animation_override_registry: marked methods are discovered");
  assert.equal(animationOverrideRegistry.intrinsic_inherited, true,
    "animation_override_registry: marked methods are discovered on subclasses");
  const lifecycleHooks = observations.mobject_lifecycle_hooks;
  assert.equal(lifecycleHooks.generate_points_returns_self, true,
    "mobject_lifecycle_hooks: generate_points is a no-op self-returning hook");
  assert.equal(lifecycleHooks.init_colors_default_returns_self, true,
    "mobject_lifecycle_hooks: init_colors default is a no-op self-returning hook");
  assert.equal(lifecycleHooks.init_colors_false_returns_self, true,
    "mobject_lifecycle_hooks: init_colors accepts propagate_colors=False");
  assert.deepEqual(lifecycleHooks.constructor_hook_order, [
    ["generate_points"],
    ["init_colors", true],
  ], "mobject_lifecycle_hooks: construction dispatches base hooks in order");
  const alwaysBuilder = observations.always_builder;
  assert.equal(alwaysBuilder.returns_builder, true,
    "always_builder: chained calls return the builder");
  assert.equal(alwaysBuilder.updater_count, 1,
    "always_builder: one updater is registered per builder call");
  assert.deepEqual(alwaysBuilder.immediate_center, [1, 0],
    "always_builder: method call runs immediately");
  assert.deepEqual(alwaysBuilder.after_updater_center, [2, 0],
    "always_builder: registered method runs on later updates");
  const setAttributes = observations.mobject_set_attributes;
  assert.equal(setAttributes.empty_set_returns_self, true,
    "mobject_set_attributes: empty set returns the receiver");
  assert.equal(setAttributes.returns_self, true,
    "mobject_set_attributes: set returns the receiver");
  assert.equal(setAttributes.name, "set-probe",
    "mobject_set_attributes: ordinary attributes are assigned");
  assert.equal(setAttributes.z_index, 2.5,
    "mobject_set_attributes: property assignments use their setter");
  assert.deepEqual(setAttributes.custom_value, { value: 7 },
    "mobject_set_attributes: arbitrary attributes retain their value");
  const basePathMethods = observations.mobject_base_path_methods;
  assert.deepEqual(basePathMethods.point_from_proportion, [
    "NotImplementedError", "Please override in a child class.",
  ], "mobject_base_path_methods: base point query remains abstract");
  assert.deepEqual(basePathMethods.proportion_from_point, [
    "NotImplementedError", "Please override in a child class.",
  ], "mobject_base_path_methods: base inverse query remains abstract");
  const baseLayoutMethods = observations.mobject_base_layout_methods;
  assert.deepEqual(baseLayoutMethods.get_point_mobject, [
    "NotImplementedError", "get_point_mobject not implemented for Mobject",
  ], "mobject_base_layout_methods: base point-mobject query remains abstract");
  assert.deepEqual(baseLayoutMethods.align_points_with_larger, [
    "NotImplementedError", "Please override in a child class.",
  ], "mobject_base_layout_methods: larger-point alignment remains abstract");
  assert.deepEqual(baseLayoutMethods.align_points_left_smaller, [true, ["left"]],
    "mobject_base_layout_methods: receiver expands when it has fewer points");
  assert.deepEqual(baseLayoutMethods.align_points_right_smaller, [true, ["right"]],
    "mobject_base_layout_methods: other expands when it has fewer points");
  assert.deepEqual(baseLayoutMethods.align_points_equal, [true, []],
    "mobject_base_layout_methods: equal point counts need no expansion");
  const depthMethods = observations.mobject_depth_methods;
  assert.equal(depthMethods.depth, 0,
    "mobject_depth_methods: planar geometry has zero depth");
  assert.equal(depthMethods.scale_returns_self, true,
    "mobject_depth_methods: scale-to-fit-depth returns the receiver");
  assert.equal(depthMethods.stretch_returns_self, true,
    "mobject_depth_methods: stretch-to-fit-depth returns the receiver");
  assert.equal(depthMethods.match_returns_self, true,
    "mobject_depth_methods: match-depth returns the receiver");
  assert.deepEqual(depthMethods.after, depthMethods.before,
    "mobject_depth_methods: zero-depth fitting leaves planar geometry unchanged");
  for (const name of ["generic_dimension_fitting", "generic_dimension_fitting_live"]) {
    const fitting = observations[name];
    assert.equal(Object.keys(fitting).length, 12, `${name}: retain every bounded case`);
    const errors = {
      negative_dimension: "AssertionError", above_range: "AssertionError",
      fractional_dimension: "IndexError", nonindex_dimension: "TypeError",
    };
    for (const [key, value] of Object.entries(fitting)) {
      assert.equal(value.error_type, errors[key] ?? null, `${name}.${key}: pinned error type`);
      assert.equal(value.returns_self, !errors[key], `${name}.${key}: receiver identity`);
      assert.deepEqual(value.target_after, value.target_before, `${name}.${key}: target is unchanged`);
      if (value.unchanged) {
        assert.deepEqual(value.after, value.before, `${name}.${key}: no mutation`);
      }
    }
    assert.ok(Math.abs(fitting.width_control.after.width - 4) <= 1e-5, `${name}: width control`);
    assert.ok(Math.abs(fitting.width_control.after.height - 2) <= 1e-5, `${name}: uniform height control`);
    assert.ok(Math.abs(fitting.height_stretch_control.after.width - 2) <= 1e-5, `${name}: stretch preserves width`);
    assert.ok(Math.abs(fitting.height_stretch_control.after.height - 3) <= 1e-5, `${name}: height control`);
  }
  for (const name of ["resource_dimension_fitting", "resource_dimension_fitting_live"]) {
    const resource = observations[name];
    assert.deepEqual(Object.keys(resource).sort(), ["family", "image"]);
    for (const value of Object.values(resource)) {
      assert.equal(value.returns_self, true, `${name}: resource fitting returns its receiver`);
      assert.ok(Math.abs(value.width - 4) <= 1e-5, `${name}: resource width`);
      assert.ok(Math.abs(value.height - 2) <= 1e-5, `${name}: resource aspect ratio`);
    }
  }
  const zIndexByZ = observations.mobject_z_index_by_z;
  assert.equal(zIndexByZ.returns_self, true,
    "mobject_z_index_by_z: returns the receiver");
  assert.equal(zIndexByZ.group_z_index, 0,
    "mobject_z_index_by_z: the planar group z position becomes its z-index");
  assert.deepEqual(zIndexByZ.member_z_indices, [0, 0],
    "mobject_z_index_by_z: default family propagation updates every member");
  for (const name of ["mobject_surround", "mobject_surround_live"]) {
    const surround = observations[name];
    assert.equal(surround.returns_self, true,
      `${name}: surround returns the receiver`);
    assert.ok(Math.abs(surround.width - 1.2) <= 1e-5,
      `${name}: width follows matched height and buffer scaling`);
    assert.ok(Math.abs(surround.height - 1.2) <= 1e-5,
      `${name}: matched height includes the buffer`);
    assertPointNear(surround.center, [0.5, 0], `${name}: centers on the target`);
  }
  assertPointNear(
    [observations.mobject_surround_live.width,
      observations.mobject_surround_live.height],
    [observations.mobject_surround.width, observations.mobject_surround.height],
    "mobject_surround_live: matches detached dimensions",
  );
  for (const name of ["circle_surround", "circle_surround_live"]) {
    const profile = observations[name];
    const diameter = Math.sqrt(20) * 1.2;
    const expected = {
      default: [diameter, diameter], height: [diameter, diameter], depth: [diameter, diameter],
      stretch: [diameter, diameter / 2], stretch_ignored_dimension: [diameter, diameter / 2],
      negative_buffer: [Math.sqrt(20) / 2, Math.sqrt(20) / 2], zero_buffer: [0, 0],
      ellipse: [diameter, diameter / 2], rotated_stretch: [diameter, diameter / 2],
      dot: [diameter, diameter], horizontal_line: [4.8, 4.8],
      vertical_line: [0, 0], vertical_height: [4.8, 4.8], vertical_stretch: [0, 4.8],
      self: [Math.sqrt(8), Math.sqrt(8)], shared_width: [Math.sqrt(85), Math.sqrt(85)],
      shared_height: [Math.sqrt(20), Math.sqrt(20)], shared_depth: [Math.sqrt(20), Math.sqrt(20)],
    };
    const expectedErrors = {
      negative_width: "AssertionError", negative_height: "AssertionError", negative_depth: "AssertionError",
      below_range: "AssertionError", above_range: "AssertionError",
      fractional_dimension: "IndexError", negative_fractional_dimension: "AssertionError",
      nonindex_dimension: "TypeError",
    };
    assert.deepEqual(Object.keys(profile).sort(), [...Object.keys(expected), ...Object.keys(expectedErrors)].sort(),
      `${name}: every success and error case observed`);
    for (const [caseName, row] of Object.entries(profile)) {
      if (caseName in expectedErrors) {
        assert.equal(row.error_type, expectedErrors[caseName], `${name}/${caseName}: pinned error class`);
        assert.equal(row.returns_self, false, `${name}/${caseName}: operation rejected`);
        assert.deepEqual(row.after, row.before, `${name}/${caseName}: receiver unchanged`);
        assert.deepEqual(row.target_after, row.target_before, `${name}/${caseName}: target unchanged`);
        assert.deepEqual(row.paint_after, row.paint_before, `${name}/${caseName}: paint unchanged`);
        continue;
      }
      assert.equal(row.error_type, null, `${name}/${caseName}: success case must not raise`);
      assert.equal(row.returns_self, true, `${name}/${caseName}: returns receiver`);
      assertPointNear([row.after.width, row.after.height], expected[caseName], `${name}/${caseName}: dimensions`);
      assert.deepEqual(row.paint_after, row.paint_before, `${name}/${caseName}: paint unchanged`);
      if (!row.shared) {
        assertPointNear(row.after.center, row.target_before.center, `${name}/${caseName}: center`);
        assert.deepEqual(row.target_after, row.target_before, `${name}/${caseName}: target unchanged`);
      }
    }
    assertPointNear(profile.shared_width.after.center, [1, 0], `${name}: shared target center after scale`);
    for (const caseName of ["shared_width", "shared_height", "shared_depth"]) {
      assert.deepEqual(profile[caseName].other_after, { center: [4, 0], width: 2, height: 1 },
        `${name}/${caseName}: unrelated member unchanged`);
    }
  }
  for (const caseName of Object.keys(observations.circle_surround)) {
    for (const key of ["before", "after", "target_before", "target_after"]) {
      const authored = observations.circle_surround[caseName][key];
      const live = observations.circle_surround_live[caseName][key];
      assertPointNear([live.width, live.height], [authored.width, authored.height],
        `circle_surround/${caseName}/${key}: authored/live dimensions agree`);
      assertPointNear(live.center, authored.center,
        `circle_surround/${caseName}/${key}: authored/live centers agree`);
    }
  }
  for (const [name, stretch] of [
    ["replace_width_live", false],
    ["replace_stretch_live", true],
  ]) {
    const profile = observations[name];
    assert.equal(profile.returns_self, true, `${name}: returns the source object`);
    assertPointNear(profile.source.center, profile.target.center,
      `${name}: centers on the target`);
    if (stretch) {
      assertPointNear([profile.source.width, profile.source.height],
        [profile.target.width, profile.target.height],
        `${name}: matches both dimensions`);
    } else {
      assert.ok(Math.abs(profile.source.height - profile.target.height) > 1e-3,
        `${name}: preserves aspect ratio while matching the selected dimension`);
      assertPointNear([profile.source.width], [profile.target.width],
        `${name}: matches the default width dimension`);
    }
  }
  for (const name of ["mobject_get_pieces", "mobject_get_pieces_live"]) {
    const profile = observations[name];
    assert.equal(profile.group_type, "Group", `${name}: pinned get_pieces returns Group`);
    assert.equal(profile.pieces.length, 4, `${name}: produces the requested piece count`);
    assert.equal(profile.empty_type, "Group", `${name}: zero pieces keep the base group class`);
    assert.equal(profile.empty_count, 0, `${name}: zero pieces yields an empty group`);
    assert.deepEqual(profile.source_after, profile.source_before,
      `${name}: building pieces does not change the source path`);
    assert.equal(profile.pieces[0].fill_opacity, 0.25,
      `${name}: copied pieces keep source fill opacity`);
    assert.equal(profile.pieces[0].stroke_width, 6,
      `${name}: copied pieces keep source stroke width`);
  }
  for (const name of [
    "mobject_z_index_reference_point",
    "mobject_z_index_reference_point_live",
  ]) {
    const profile = observations[name];
    assertPointNear(profile.default_reference, profile.source_center,
      `${name}: defaults to the receiver center`);
    assertPointNear(profile.grouped_reference, profile.reference_center,
      `${name}: follows the explicit z_index_group center`);
  }
  for (const name of [
    "mobject_add_background_rectangle",
    "mobject_add_background_rectangle_live",
  ]) {
    const profile = observations[name];
    assert.equal(profile.returns_self, true, `${name}: returns the family object`);
    assert.equal(profile.background_is_first, true,
      `${name}: prepends the generated background rectangle`);
    assert.equal(profile.member_count, 3, `${name}: retains existing family members`);
    assert.ok(Math.abs(profile.background.width - 2.9) <= 1e-5,
      `${name}: background width includes horizontal buffer`);
    assert.ok(Math.abs(profile.background.height - 1.1) <= 1e-5,
      `${name}: background height includes vertical buffer`);
    assert.ok(Math.abs(profile.fill_opacity - 0.4) <= 1e-5,
      `${name}: applies the requested fill opacity`);
  }
  for (const name of ["mobject_match_z", "mobject_match_z_live"]) {
    const profile = observations[name];
    assert.equal(profile.match_returns_self, true, `${name}: match_z returns the receiver`);
    assert.equal(profile.set_returns_self, true, `${name}: set_coord returns the receiver`);
    assert.deepEqual(profile.after_match, profile.before,
      `${name}: matching planar z does not move or resize the receiver`);
    assert.deepEqual(profile.after_set, profile.before,
      `${name}: setting planar z to zero does not move or resize the receiver`);
    assert.equal(profile.receiver_z, 0, `${name}: receiver remains on the plane`);
    assert.equal(profile.target_z, 0, `${name}: target remains on the plane`);
  }
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
