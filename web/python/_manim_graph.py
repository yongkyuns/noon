"""Thin Graph/DiGraph adapters over the shared Rust authoring handle.

Python owns only its arbitrary hashable-key to u32 mapping. Rust owns graph
topology, arrow endpoints, persistent placements, and publication.
"""

from __future__ import annotations

from collections.abc import Hashable, Mapping, Sequence
import math
from numbers import Integral

import _manim_compat as _compat
import _manim_semantic_handles as _shared
import noon as _base
from _noon_errors import engine_call

try:
    from js import noonAuthoringGraphOptions as _graph_options
    from pyodide.ffi import to_js as _to_js
except ImportError:  # pragma: no cover - native import smoke only
    _graph_options = None
    _to_js = None

_LAYOUT_SCALE_UNSET = object()


def _array(values):
    if _to_js is None:
        raise RuntimeError("Graph requires the shared Rust authoring host")
    return _to_js(list(values))


def _point(value):
    point = _base._as_vec2(value)
    return float(point.x), float(point.y)


def _layout_scale_pair(value):
    """Return independent x/y scale values accepted by Manim's 2D layouts."""
    if isinstance(value, Sequence) and not isinstance(value, (str, bytes)):
        if len(value) != 2:
            raise ValueError("2D layout_scale tuples must contain x and y values")
        return (
            _shared._ir._finite_number("layout_scale[0]", value[0]),
            _shared._ir._finite_number("layout_scale[1]", value[1]),
        )
    return _shared._ir._finite_number("layout_scale", value), None


_VERTEX_CONFIG_KEYS = frozenset({
    "color", "fill_color", "stroke_color", "stroke_width", "stroke_opacity",
    "radius", "fill_opacity",
})
_EDGE_CONFIG_KEYS = frozenset({"color", "stroke_color", "stroke_width", "stroke_opacity"})


def _split_style_config(name, value, supported, entity_keys, per_key_supported=None):
    if value is None:
        return {}, {}
    if not isinstance(value, Mapping):
        raise TypeError(f"{name} must be a mapping")
    global_values, per_key = {}, {}
    for key, option in dict(value).items():
        if key in entity_keys:
            if not isinstance(option, Mapping):
                raise TypeError(f"{name}[{key!r}] must be a mapping of style options")
            per_key[key] = _validated_key_style(name, option, per_key_supported)
        elif isinstance(key, str) and key in supported:
            global_values[key] = option
        elif isinstance(key, tuple) and name == "edge_config":
            raise ValueError(f"edge_config references unknown edge {key!r}")
        elif not isinstance(key, str):
            raise NotImplementedError(f"per-key {name} key {key!r} is not present in this graph")
        else:
            raise TypeError(f"unsupported {name} option: {key}")
    return global_values, per_key


def _validated_key_style(name, value, supported=None):
    if supported is None:
        supported = {
            "color", "fill_color", "stroke_color", "fill_opacity", "stroke_width",
            "stroke_opacity",
        }
    unknown = [key for key in value if key not in supported]
    if unknown:
        raise NotImplementedError(
            f"unsupported per-key {name} style option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    result = dict(value)
    for key in ("color", "fill_color", "stroke_color"):
        if key in result:
            result[key] = _compat._as_color(f"{name}.{key}", result[key])
    if "fill_opacity" in result:
        result["fill_opacity"] = _compat._opacity(
            f"{name}.fill_opacity", result["fill_opacity"]
        )
    if "stroke_opacity" in result:
        result["stroke_opacity"] = _compat._opacity(
            f"{name}.stroke_opacity", result["stroke_opacity"]
        )
    if "stroke_width" in result:
        _compat._manim_stroke_width(result["stroke_width"])
    if "radius" in result:
        result["radius"] = _shared._ir._finite_number(f"{name}.radius", result["radius"])
        if result["radius"] <= 0:
            raise ValueError(f"{name}.radius must be positive")
    return result


def _apply_key_style(target, values):
    fill_color = values.get("fill_color", values.get("color"))
    stroke_color = values.get("stroke_color", values.get("color"))
    fill_opacity = values.get("fill_opacity")
    stroke_width = values.get("stroke_width")
    stroke_opacity = values.get("stroke_opacity")
    if (
        fill_color is None
        and stroke_color is None
        and fill_opacity is None
        and stroke_width is None
        and stroke_opacity is None
    ):
        return
    target.set_style(
        fill_color=fill_color,
        fill_opacity=fill_opacity,
        stroke_color=stroke_color,
        stroke_width=stroke_width,
        stroke_opacity=stroke_opacity,
    )


def _set_color(method, name, value):
    color = _compat._as_color(name, value)
    engine_call(method, color.red, color.green, color.blue, color.alpha,
                operation="Graph.style")


def _color_components(name, value):
    color = _compat._as_color(name, value)
    return color.red, color.green, color.blue, color.alpha


def _graph_vertex_type(vertex_type):
    from _manim_geometry import Dot

    selected = Dot if vertex_type is None else vertex_type
    if selected not in (Dot, _compat.Circle):
        raise NotImplementedError(
            "Graph currently supports Dot and Circle vertex_type values"
        )
    return selected


def _graph_labels(labels, vertices, label_fill_color):
    """Build the supported label subset using retained Mobjects."""
    if isinstance(labels, bool):
        requested = {key: True for key in vertices} if labels else {}
    elif isinstance(labels, Mapping):
        requested = {key: label for key, label in labels.items() if key in vertices}
    else:
        raise TypeError("labels must be a bool or a mapping of vertex keys to labels")

    if not requested:
        return {}

    default_color = (
        _compat._as_color("label_fill_color", label_fill_color)
        if any(value is True for value in requested.values())
        else _base.BLACK
    )
    result = {}
    for key, label in requested.items():
        if label is None or label is False:
            continue
        color = default_color if label is True else _base.BLACK
        if label is True:
            label = _base.MathTex(str(key), color=color)
        elif isinstance(label, str):
            label = _base.MathTex(label, color=color)
        elif not isinstance(label, _base.Mobject):
            raise TypeError(
                f"labels[{key!r}] must be a string, bool, Mobject, or None"
            )
        result[key] = label
    return result


def _labeled_dot_radius(label):
    return _base.SMALL_BUFF + math.hypot(float(label.width), float(label.height)) / 2


def _default_vertex_style(circle_vertices, label=None):
    if circle_vertices:
        color = _compat._as_color("vertex default color", "#FC6255")
        radius, fill_opacity, stroke_width = 1.0, 0.0, 4.0
    else:
        color = _compat._as_color("vertex default color", "#FFFFFF")
        radius = _labeled_dot_radius(label) if label is not None else 0.08
        fill_opacity, stroke_width = 1.0, 0.0
    return {
        "radius": radius,
        "fill_color": color,
        "fill_opacity": fill_opacity,
        "stroke_color": color,
        "stroke_width": stroke_width,
        "stroke_opacity": 1.0,
    }


def _complete_keyed_vertex_styles(keyed_config, explicit_keys, labels, circle_vertices):
    for key in explicit_keys:
        style = dict(keyed_config[key])
        color = style.pop("color", None)
        if color is not None:
            style.setdefault("fill_color", color)
            style.setdefault("stroke_color", color)
        keyed_config[key] = {
            **_default_vertex_style(circle_vertices, labels.get(key)),
            **style,
        }


def _complete_keyed_edge_styles(keyed_config):
    default_color = _compat._as_color("edge default color", "#FFFFFF")
    for edge, raw_style in list(keyed_config.items()):
        style = dict(raw_style)
        color = style.pop("color", None)
        if color is not None:
            style.setdefault("stroke_color", color)
        keyed_config[edge] = {
            "stroke_color": default_color,
            "stroke_width": _compat.MANIM_DEFAULT_STROKE_WIDTH,
            "stroke_opacity": 1.0,
            **style,
        }


def _graph_style_options(vertex_config, edge_config):
    if _graph_options is None:
        raise RuntimeError("Graph requires the shared Rust authoring host")
    vertices = vertex_config
    edges = edge_config
    options = engine_call(_graph_options, operation="Graph.style")
    try:
        if "color" in vertices:
            _set_color(options.setVertexFill, "vertex_config.color", vertices["color"])
            _set_color(options.setVertexStroke, "vertex_config.color", vertices["color"])
        if "fill_color" in vertices:
            _set_color(options.setVertexFill, "vertex_config.fill_color", vertices["fill_color"])
        if "stroke_color" in vertices:
            _set_color(options.setVertexStroke, "vertex_config.stroke_color", vertices["stroke_color"])
        if "radius" in vertices:
            engine_call(options.setVertexRadius, float(vertices["radius"]), operation="Graph.style")
        if "fill_opacity" in vertices:
            engine_call(options.setVertexFillOpacity, float(vertices["fill_opacity"]), operation="Graph.style")
        if "stroke_width" in vertices:
            engine_call(options.setVertexStrokeWidth, float(vertices["stroke_width"]), operation="Graph.style")
        if "stroke_opacity" in vertices:
            engine_call(
                options.setVertexStrokeOpacity,
                float(vertices["stroke_opacity"]),
                operation="Graph.style",
            )
        color = edges.get("stroke_color", edges.get("color"))
        if color is not None:
            _set_color(options.setEdgeColor, "edge_config.color", color)
        if "stroke_width" in edges:
            engine_call(options.setEdgeStrokeWidth, float(edges["stroke_width"]), operation="Graph.style")
        if "stroke_opacity" in edges:
            engine_call(
                options.setEdgeStrokeOpacity,
                float(edges["stroke_opacity"]),
                operation="Graph.style",
            )
    except BaseException:
        options.free()
        raise
    return options


def _vertex_style_override_rows(vertices, keyed_config):
    missing = float("nan")
    rows = []
    for key in vertices:
        style = keyed_config.get(key, {})
        fill = style.get("fill_color", style.get("color"))
        stroke = style.get("stroke_color", style.get("color"))
        fill_components = (
            _color_components("vertex_config.fill_color", fill)
            if fill is not None else (missing,) * 4
        )
        stroke_components = (
            _color_components("vertex_config.stroke_color", stroke)
            if stroke is not None else (missing,) * 4
        )
        rows.extend((
            style.get("radius", missing),
            *fill_components,
            style.get("fill_opacity", missing),
            *stroke_components,
            style.get("stroke_width", missing),
            style.get("stroke_opacity", missing),
        ))
    return rows


def _edge_style_override_rows(edges, keyed_config):
    missing = float("nan")
    rows = []
    for edge in edges:
        style = keyed_config.get(edge, {})
        color = style.get("stroke_color", style.get("color"))
        color_components = (
            _color_components("edge_config.color", color)
            if color is not None else (missing,) * 4
        )
        rows.extend((
            *color_components,
            style.get("stroke_width", missing),
            style.get("stroke_opacity", missing),
        ))
    return rows


def _coerce_graph_edges(edges):
    """Split NetworkX-style edge tuples from their retained topology keys."""
    topology, attributes = [], []
    for index, edge in enumerate(edges):
        if isinstance(edge, (str, bytes)):
            raise TypeError(f"Graph edge {index} must be a two- or three-item sequence")
        try:
            row = tuple(edge)
        except TypeError as error:
            raise TypeError(f"Graph edge {index} must be a two- or three-item sequence") from error
        if len(row) == 2:
            endpoints, data = row, {}
        elif len(row) == 3:
            endpoints = row[:2]
            if not isinstance(row[2], Mapping):
                raise TypeError(f"Graph edge {index} attributes must be a mapping")
            data = dict(row[2])
        else:
            raise ValueError(f"Graph edge {index} must contain two endpoints and optional attributes")
        topology.append(tuple(endpoints))
        attributes.append(data)
    return topology, attributes


class _GraphBase(_compat.Group):
    """Shared retained Graph/DiGraph facade.

    The default spring layout accepts NetworkX's ``seed``, ``pos``, ``fixed``,
    ``k``, ``iterations``, ``threshold``, ``weight``, ``center``, and ``dim=2``
    configuration. NumPy prepares NetworkX-compatible initial positions while
    Rust owns force integration. Circular layouts accept ``center`` and ``dim=2``
    through ``layout_config``; random layouts accept ``seed``, ``center``, and
    ``dim=2``. Kamada-Kawai layouts accept ``dist``, ``pos``, ``weight``,
    ``center``, and ``dim=2``; Rust owns path-distance preparation and cost
    minimization. Shell layouts accept ``nlist``, ``rotate``,
    ``center``, and ``dim=2``; spiral layouts accept ``resolution``,
    ``equidistant``, ``center``, and ``dim=2``; partite layouts accept
    ``partitions``, ``align``, and ``center``; tree layouts accept
    ``root_vertex``, ``orientation``, and ``vertex_spacing``. Spectral layouts
    accept NetworkX's ``weight``, ``center``, and ``dim=2`` configuration; Rust
    computes the Laplacian eigenvectors and retains the resulting positions.
    Other options remain unsupported until their semantics are qualified. Two-axis
    ``layout_scale`` tuples are supported for spring, random, circular, spiral,
    partite, spectral, Kamada-Kawai, and tree layouts; shell follows NetworkX's scalar-only
    scale contract. Partite layouts accept ``partitions`` directly or in
    ``layout_config``; the latter takes precedence. Constructor
    ``vertex_config`` and ``edge_config`` accept per-key color, fill, stroke,
    fill-opacity, stroke-width, and stroke-opacity overrides for vertices and
    edges present at construction time. ``add_vertices`` inherits
    constructor-wide vertex styles and accepts the same per-key style subset
    plus radius in its atomic mutation. ``add_edges`` inherits constructor-wide
    edge styles and accepts per-edge color, stroke-color, stroke-width, and
    stroke-opacity overrides. Constructor ``vertex_config`` also accepts
    per-key radius. The standard ``Circle`` class is accepted as a constructor
    or ``add_vertices`` vertex type using retained circle geometry and Manim's
    Circle defaults; ``add_edges``-created endpoint vertices use Dot. Arbitrary
    custom mobject types remain unsupported. Constructor ``labels`` accepts a
    boolean or a partial vertex-to-label mapping; labels may be booleans,
    strings, or retained Mobjects. Generated labels use MathTex and
    ``label_fill_color``; default Dot radii follow Manim's LabeledDot bounds.
    Label mobjects stay in the shared family and are preserved by layout
    changes, copies, incremental vertex creation, and vertex removal.
    """

    # This typed vector wrapper retains the shared family-paint contract.
    set_opacity = _compat.VGroup.set_opacity
    set_fill = _compat.VGroup.set_fill
    set_stroke = _compat.VGroup.set_stroke

    _directed = False

    def __init__(self, vertices, edges, layout="spring", *, layout_scale=2.0,
                 layout_center=(0.0, 0.0), layout_config=None, **kwargs):
        labels = kwargs.pop("labels", False)
        label_fill_color = kwargs.pop("label_fill_color", _base.BLACK)
        vertex_config = kwargs.pop("vertex_config", None)
        edge_config = kwargs.pop("edge_config", None)
        vertex_type = kwargs.pop("vertex_type", None)
        root_vertex = kwargs.pop("root_vertex", None)
        partitions = kwargs.pop("partitions", None)
        if kwargs:
            raise TypeError("unsupported Graph option(s): " + ", ".join(sorted(kwargs)))
        selected_vertex_type = _graph_vertex_type(vertex_type)
        circle_vertices = selected_vertex_type is _compat.Circle
        vertex_mapping = vertices if isinstance(vertices, Mapping) else None
        vertices = tuple(vertices)
        label_mobjects = _graph_labels(labels, vertices, label_fill_color)
        if label_mobjects and circle_vertices:
            raise TypeError("Circle vertices do not accept labels; use the default Dot vertex_type")
        edges, edge_data = _coerce_graph_edges(edges)
        explicit = layout if isinstance(layout, Mapping) else (
            vertex_mapping if layout in {None, "explicit"} and vertex_mapping is not None else None
        )
        spring = isinstance(layout, str) and layout == "spring"
        random_layout = isinstance(layout, str) and layout == "random"
        tree = isinstance(layout, str) and layout == "tree"
        circular = isinstance(layout, str) and layout == "circular"
        shell = isinstance(layout, str) and layout == "shell"
        spiral = isinstance(layout, str) and layout == "spiral"
        partite = isinstance(layout, str) and layout == "partite"
        spectral = isinstance(layout, str) and layout == "spectral"
        kamada_kawai = isinstance(layout, str) and layout == "kamada_kawai"
        if root_vertex is not None and not tree:
            raise NotImplementedError("root_vertex is supported only by the tree layout")
        if partitions is not None and not partite:
            raise NotImplementedError("partitions are supported only by the partite layout")
        if (explicit is None and not spring and not random_layout and not tree
                and not circular and not shell and not spiral and not partite
                and not spectral and not kamada_kawai):
            raise NotImplementedError("Graph requires a qualified layout or a complete position mapping")
        self._graph_key_ids = {}
        self._next_graph_id = 0
        positions = []
        for key in vertices:
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids:
                raise ValueError("duplicate Graph vertex key")
            self._graph_key_ids[key] = self._next_graph_id
            self._next_graph_id += 1
            if explicit is not None:
                try:
                    positions.extend(_point(explicit[key]))
                except KeyError as error:
                    raise ValueError("Graph layout must cover every vertex exactly once") from error
        if explicit is not None and len(explicit) != len(self._graph_key_ids):
            raise ValueError("Graph layout must cover every vertex exactly once")
        edge_ids = self._edge_ids(edges)
        self._edge_attributes = {self._edge_attribute_key(edge): attributes
                                 for edge, attributes in zip(edges, edge_data)}
        global_vertex_config, keyed_vertex_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, set(self._graph_key_ids),
            per_key_supported=_VERTEX_CONFIG_KEYS,
        )
        explicit_keyed_vertex_ids = tuple(keyed_vertex_config)
        if label_mobjects and not circle_vertices:
            for key, label in label_mobjects.items():
                keyed_style = keyed_vertex_config.get(key)
                if keyed_style is not None and "radius" not in keyed_style:
                    keyed_style["radius"] = _labeled_dot_radius(label)
                elif keyed_style is None and "radius" not in global_vertex_config:
                    keyed_vertex_config.setdefault(key, {})["radius"] = _labeled_dot_radius(label)
        self._default_vertex_config = dict(global_vertex_config)
        if circle_vertices:
            # Match Circle's defaults while retaining the same Rust-owned
            # circle primitives used by Dot vertices. Late additions retain
            # their own default Dot semantics unless Circle is requested.
            default_circle_color = _compat._as_color(
                "vertex default color", "#FC6255"
            )
            circle_defaults = {
                "radius": 1.0,
                "fill_color": default_circle_color,
                "fill_opacity": 0.0,
                "stroke_color": default_circle_color,
                "stroke_width": 4.0,
                "stroke_opacity": 1.0,
            }
            if "color" in global_vertex_config:
                circle_defaults["fill_color"] = global_vertex_config["color"]
                circle_defaults["stroke_color"] = global_vertex_config["color"]
            circle_defaults.update(global_vertex_config)
            global_vertex_config = circle_defaults
        _complete_keyed_vertex_styles(
            keyed_vertex_config, explicit_keyed_vertex_ids, label_mobjects,
            circle_vertices,
        )
        global_edge_config, keyed_edge_config = _split_style_config(
            "edge_config", edge_config, _EDGE_CONFIG_KEYS, set(edges)
        )
        _complete_keyed_edge_styles(keyed_edge_config)
        self._default_edge_config = dict(global_edge_config)
        center = (0.0, 0.0)
        if circular:
            center = _circular_layout_center(layout_config, layout_center)
        shell_groups, rotate = ([], None)
        resolution, equidistant = (0.35, False)
        partite_groups, align_horizontal = ([], False)
        spring_positions, fixed_indices, spring_k = ([], [], None)
        random_positions = []
        tree_root, tree_orientation_up, tree_spacing = (None, False, None)
        kamada_kawai_distances, kamada_kawai_positions = ([], [])
        scale_y = None
        spring_iterations, spring_threshold, spring_rescale = (50, 1e-4, True)
        spring_fixed_is_set = False
        if spring:
            (center, spring_positions, fixed_indices, spring_k,
             spring_iterations, spring_threshold, spring_rescale,
             scale, scale_y, spring_fixed_is_set) = _spring_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
            spring_edge_weights = self._layout_edge_weights(layout_config, edges=edges)
        elif random_layout:
            center, random_positions, scale, scale_y = _random_layout_config(
                layout_config, layout_center, self._graph_key_ids, layout_scale
            )
        elif tree:
            tree_root, tree_orientation_up, tree_spacing, scale, scale_y = _tree_layout_config(
                layout_config, self._graph_key_ids, layout_scale, self._directed,
                root_vertex=root_vertex,
            )
        elif shell:
            center, shell_groups, rotate = _shell_layout_config(
                layout_config, layout_center, self._graph_key_ids
            )
        elif spiral:
            center, resolution, equidistant = _spiral_layout_config(
                layout_config, layout_center, len(self._graph_key_ids)
            )
        elif partite:
            center, partite_groups, align_horizontal = _partite_layout_config(
                layout_config, layout_center, self._graph_key_ids,
                partitions=partitions,
            )
        elif spectral:
            center, scale, scale_y = _spectral_layout_config(
                layout_config, layout_center, layout_scale,
            )
            spectral_edge_weights = self._layout_edge_weights(layout_config, edges=edges)
        elif kamada_kawai:
            center, kamada_kawai_distances, kamada_kawai_positions = _kamada_kawai_layout_config(
                layout_config, layout_center, self._graph_key_ids,
            )
            kamada_kawai_edge_weights = self._layout_edge_weights(
                layout_config, edges=edges, use_weight=not kamada_kawai_distances
            )
        if not spring and not random_layout and not tree:
            if shell:
                if isinstance(layout_scale, Sequence) and not isinstance(layout_scale, (str, bytes)):
                    raise NotImplementedError("shell layout_scale must be a scalar")
                scale = _shared._ir._finite_number("layout_scale", layout_scale)
            else:
                scale, scale_y = _layout_scale_pair(layout_scale)
        context = _shared._live_constructor_context("Graph", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Graph construction requires a canonical Scene authoring context")
        options = _graph_style_options(global_vertex_config, global_edge_config)
        ids = _array(self._graph_key_ids.values())

        def create_graph(method, *args):
            nonlocal options
            # wasm-bindgen consumes this by-value wrapper, including on Err.
            transferred_options = options
            options = None
            return engine_call(
                method, *args, transferred_options, operation="Graph.create"
            )

        try:
            for key, style in keyed_vertex_config.items():
                if "radius" in style:
                    engine_call(
                        options.setVertexRadiusFor,
                        self._graph_key_ids[key],
                        float(style["radius"]),
                        operation="Graph.style",
                    )
            if spring:
                self._graph_handle = create_graph(
                    context.liveCreateSpringGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(spring_edge_weights),
                    _array(value for point in spring_positions for value in point),
                    _array(fixed_indices), spring_fixed_is_set,
                    scale, scale_y, *center, spring_k, spring_iterations, spring_threshold,
                    spring_rescale,
                )
            elif random_layout:
                self._graph_handle = create_graph(
                    context.liveCreateRandomGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(value for point in random_positions for value in point),
                    scale, scale_y,
                )
            elif tree:
                spacing_x, spacing_y = (None, None) if tree_spacing is None else tree_spacing
                self._graph_handle = create_graph(
                    context.liveCreateTreeGraph,
                    bool(self._directed), ids, _array(edge_ids), tree_root,
                    tree_orientation_up, spacing_x, spacing_y, scale, scale_y,
                )
            elif circular:
                self._graph_handle = create_graph(
                    context.liveCreateCircularGraph, bool(self._directed), ids, _array(edge_ids),
                    scale, scale_y, *center,
                )
            elif shell:
                shell_indices, shell_sizes = _flatten_groups(shell_groups)
                self._graph_handle = create_graph(
                    context.liveCreateShellGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(shell_indices), _array(shell_sizes), scale, *center, rotate,
                )
            elif spiral:
                self._graph_handle = create_graph(
                    context.liveCreateSpiralGraph,
                    bool(self._directed), ids, _array(edge_ids), scale, scale_y, *center,
                    resolution, equidistant,
                )
            elif partite:
                partition_indices, partition_sizes = _flatten_groups(partite_groups)
                self._graph_handle = create_graph(
                    context.liveCreatePartiteGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(partition_indices), _array(partition_sizes),
                    scale, scale_y, *center, align_horizontal,
                )
            elif spectral:
                self._graph_handle = create_graph(
                    context.liveCreateSpectralGraph, bool(self._directed), ids,
                    _array(edge_ids), _array(spectral_edge_weights), scale, scale_y, *center,
                )
            elif kamada_kawai:
                self._graph_handle = create_graph(
                    context.liveCreateKamadaKawaiGraph,
                    bool(self._directed), ids, _array(edge_ids),
                    _array(kamada_kawai_edge_weights),
                    _array(kamada_kawai_distances),
                    _array(value for point in kamada_kawai_positions for value in point),
                    scale, scale_y, *center,
                )
            else:
                self._graph_handle = create_graph(
                    context.liveCreateGraph, bool(self._directed), ids, _array(positions),
                    _array(edge_ids),
                )
        except BaseException:
            if options is not None:
                options.free()
            raise
        self._edge_wrappers = {}
        self._edge_mapping = None
        self._edge_mapping_sources = {}
        self._labels = label_mobjects
        self.labels = self._labels
        self._vertex_wrappers = {}
        self._canonical_live_target_context = context
        # Graph roots own topology and reject generic membership edits. Keep
        # labels as siblings in an ordinary retained Group around that root.
        graph_wrapper = object.__new__(_compat.Group)
        _shared._attach_shared_family(
            graph_wrapper, engine_call(self._graph_handle.family), context
        )
        graph_wrapper._canonical_live_target_context = context
        self._graph_family_wrapper = graph_wrapper
        graph_wrapper._refresh_semantic_members = self._refresh_graph_family_members
        for key, label in self._labels.items():
            label.move_to(self._raw_vertex(key).get_center())
        _compat.Group.__init__(self, graph_wrapper, *self._labels.values())
        for key, style in keyed_vertex_config.items():
            _apply_key_style(self._raw_vertex(key), style)
        for edge, style in keyed_edge_config.items():
            _apply_key_style(self._edge_style_target(edge), style)

    def _edge_ids(self, edges):
        values = []
        for start, end in edges:
            try:
                values.extend((self._graph_key_ids[start], self._graph_key_ids[end]))
            except KeyError as error:
                raise ValueError("Graph edge references an unknown vertex key") from error
        return values

    def _edge_attribute_key(self, edge):
        start, end = (self._graph_key_ids[key] for key in edge)
        return (start, end) if self._directed or start <= end else (end, start)

    def _canonical_edge_keys(self):
        """Adapt one authoritative Rust projection; Python retains host keys only."""
        endpoints = list(engine_call(self._graph_handle.edgeKeys, operation="Graph.edges"))
        if len(endpoints) % 2:
            raise RuntimeError("Rust Graph returned an incomplete edge projection")
        keys = {value: key for key, value in self._graph_key_ids.items()}
        try:
            return [(keys[int(endpoints[index])], keys[int(endpoints[index + 1])])
                    for index in range(0, len(endpoints), 2)]
        except KeyError as error:
            raise RuntimeError("Rust Graph returned an unbound Python key") from error

    def _layout_edge_weights(self, layout_config, *, use_weight=True, edges=None):
        ordered = self._canonical_edge_keys() if edges is None else edges
        options = {} if layout_config is None else layout_config
        weight = options.get("weight", "weight") if use_weight else None
        if weight is None:
            return [1.0] * len(ordered)
        return [
            _shared._ir._finite_number(
                f"edge data {weight!r}", self._edge_attributes.get(
                    self._edge_attribute_key(edge), {}).get(weight, 1.0)
            )
            for edge in ordered
        ]

    def _cache_added_edges(self, edges):
        # These keys were accepted by the shared atomic Rust mutation. Refresh
        # only affected host-wrapper cache entries, never the whole graph.
        for edge in edges:
            self._edge_attributes.setdefault(self._edge_attribute_key(edge), {})
            if self._edge_mapping is not None:
                wrapper = self._edge_style_target(edge)
                self._edge_mapping[edge] = wrapper
                self._edge_mapping_sources[edge] = wrapper

    def _cache_removed_edges(self, edges):
        for edge in edges:
            key = self._edge_attribute_key(edge)
            self._edge_attributes.pop(key, None)
            self._edge_wrappers.pop(key, None)
            candidates = (edge,) if self._directed else (edge, (edge[1], edge[0]))
            for candidate in candidates:
                if candidate in self._edge_mapping_sources:
                    self._edge_mapping_sources.pop(candidate)
                    if self._edge_mapping is not None:
                        self._edge_mapping.pop(candidate, None)
                    break

    def _edge_style_target(self, edge):
        if len(edge) != 2:
            raise ValueError("edge_config keys must be two-vertex tuples")
        try:
            start, end = (self._graph_key_ids[key] for key in edge)
        except KeyError as error:
            raise ValueError(f"edge_config references unknown vertex {error.args[0]!r}") from error
        cache_key = self._edge_attribute_key(edge)
        family = engine_call(self._graph_handle.edge, start, end, operation="Graph.edge")
        if family is None:
            raise ValueError(f"edge_config references unknown edge {edge!r}")
        cached = self._edge_wrappers.get(cache_key)
        if cached is not None and _shared._shared_handle_key(
                cached._semantic_family_handle) == _shared._shared_handle_key(family):
            return cached
        target = object.__new__(_compat.Group)
        _shared._attach_shared_family(target, family, self._context())
        target._canonical_live_target_context = self._context()
        self._edge_wrappers[cache_key] = target
        return target

    def _context(self):
        return self._canonical_live_target_context

    def _refresh_graph_family_members(self):
        """Refresh topology wrappers from the graph's current Rust family."""
        _shared._attach_shared_family(
            self._graph_family_wrapper,
            engine_call(self._graph_handle.family),
            self._context(),
        )

    def _raw_vertex(self, key):
        try:
            vertex_id = self._graph_key_ids[key]
        except (KeyError, TypeError) as error:
            raise ValueError(f"Could not find {key} in vertices or edges") from error
        handle = engine_call(self._graph_handle.vertex, vertex_id)
        if handle is None:
            raise ValueError(f"Could not find {key} in vertices or edges")
        identity = f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"
        member = self._graph_family_wrapper._semantic_member_wrappers.get(identity)
        if member is None:
            member = object.__new__(_compat.VMobject)
            _shared._attach_shared_handle(member, handle)
            member._canonical_live_target_context = self._context()
            self._graph_family_wrapper._semantic_member_wrappers[identity] = member
        return member

    def __getitem__(self, key):
        """Resolve a vertex key or edge tuple to its retained semantic Mobject."""
        try:
            is_vertex = key in self._graph_key_ids
        except TypeError:
            is_vertex = False
        if is_vertex:
            vertex = self._raw_vertex(key)
            label = self._labels.get(key)
            if label is None:
                return vertex
            wrapper = self._vertex_wrappers.get(key)
            if wrapper is None:
                wrapper = _compat.VGroup(vertex, label)
                self._vertex_wrappers[key] = wrapper
            return wrapper
        if isinstance(key, tuple) and len(key) == 2:
            try:
                return self._edge_style_target(key)
            except ValueError:
                pass
        raise ValueError(f"Could not find {key} in vertices or edges")

    @property
    def vertices(self):
        return {key: self[key] for key in self._graph_key_ids}

    @property
    def edges(self):
        """Retain a normal mapping while Rust owns membership and edge order."""
        ordered = self._canonical_edge_keys()
        current = {edge: self._edge_style_target(edge) for edge in ordered}
        if self._edge_mapping is None:
            self._edge_mapping = dict(current)
        else:
            for edge in self._edge_mapping_sources.keys() - current.keys():
                self._edge_mapping.pop(edge, None)
            for edge, wrapper in current.items():
                if self._edge_mapping_sources.get(edge) is not wrapper:
                    self._edge_mapping[edge] = wrapper
        self._edge_mapping_sources = current
        return self._edge_mapping

    def add_vertices(self, *vertices, positions=None, labels=False,
                     label_fill_color=_base.BLACK, vertex_type=None, vertex_config=None):
        """Append vertices, optionally supplying positions and per-key styles.

        ``vertex_type`` supports the standard Dot and Circle classes.
        ``vertex_config`` uses the constructor's global style keys and accepts
        per-key radius, color, fill-opacity, stroke-width, and stroke-opacity
        overrides. Global
        values extend constructor defaults; per-key values override those
        globals in the same graph publication. A mapping passed as the sole
        positional argument remains a shorthand for ``positions``.
        """
        if len(vertices) == 1 and isinstance(vertices[0], Mapping):
            if positions is not None:
                raise TypeError("positions cannot be combined with a vertex-to-position mapping")
            positions = vertices[0]
            vertices = tuple(positions)
        if positions is not None and not isinstance(positions, Mapping):
            raise TypeError("positions must be a mapping of vertex keys to positions")
        if not vertices:
            return []
        positions_by_key = positions if isinstance(positions, Mapping) else {}
        pending, ids, flat_positions = [], [], []
        next_id = self._next_graph_id
        center = _point(self.get_center())
        for key in vertices:
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids or any(key == item[0] for item in pending):
                raise ValueError("duplicate Graph vertex key")
            x, y = _point(positions_by_key.get(key, center))
            pending.append((key, x, y))
            ids.append(next_id)
            next_id += 1
            flat_positions.extend((x, y))
        pending_keys = {key for key, _, _ in pending}
        global_config, keyed_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, pending_keys,
            per_key_supported={
                "color", "fill_color", "stroke_color", "fill_opacity",
                "stroke_width", "stroke_opacity", "radius",
            },
        )
        explicit_keyed_vertex_ids = tuple(keyed_config)
        label_mobjects = _graph_labels(
            labels, tuple(key for key, _, _ in pending), label_fill_color
        )
        selected_vertex_type = _graph_vertex_type(vertex_type)
        circle_vertices = selected_vertex_type is _compat.Circle
        if label_mobjects and circle_vertices:
            raise TypeError("Circle vertices do not accept labels; use the default Dot vertex_type")
        if label_mobjects and not circle_vertices:
            for key, label in label_mobjects.items():
                if (
                    "radius" not in global_config
                    and "radius" not in keyed_config.get(key, {})
                ):
                    keyed_config.setdefault(key, {})["radius"] = _labeled_dot_radius(label)
        _complete_keyed_vertex_styles(
            keyed_config, explicit_keyed_vertex_ids, label_mobjects,
            circle_vertices,
        )
        base_config = _default_vertex_style(True) if circle_vertices else {}
        base_config.update(self._default_vertex_config)
        base_config.update(global_config)
        options = _graph_style_options(base_config, {})
        try:
            style_overrides = _vertex_style_override_rows(
                (key for key, _, _ in pending), keyed_config
            )
            vertex_ids = _array(ids)
            vertex_positions = _array(flat_positions)
            vertex_styles = _array(style_overrides)
            transferred_options = options
            options = None
            engine_call(
                self._context().liveGraphAddVerticesWithOptions,
                self._graph_handle,
                vertex_ids,
                vertex_positions,
                vertex_styles,
                transferred_options,
                operation="Graph.add_vertices",
            )
        except BaseException:
            if options is not None:
                options.free()
            raise
        if label_mobjects:
            positions_by_key = {key: (x, y) for key, x, y in pending}
            for key, label in label_mobjects.items():
                label.move_to(positions_by_key[key])
            try:
                self.add(*label_mobjects.values())
            except BaseException:
                engine_call(
                    self._context().liveGraphRemoveVertices,
                    self._graph_handle,
                    _array(ids),
                    operation="Graph.add_vertices.rollback",
                )
                raise
            self._labels.update(label_mobjects)
        self._graph_key_ids.update((key, vertex_id) for (key, _, _), vertex_id in zip(pending, ids))
        self._next_graph_id = next_id
        return [self[key] for key, _, _ in pending]

    def add_edges(self, *edges, edge_config=None, positions=None, vertex_config=None):
        """Add edges and create any missing endpoint vertices atomically.

        Missing vertices default to the current graph center. ``positions`` and
        ``vertex_config`` customize those vertices; edge styles continue to
        inherit graph defaults and accept per-edge overrides.
        """
        if not edges:
            return _compat.VGroup()
        if len(edges) == 1:
            argument = edges[0]
            edge_batch = (
                isinstance(argument, tuple)
                and len(argument) == 2
                and all(isinstance(edge, (tuple, list)) and len(edge) == 2 for edge in argument)
            )
            if edge_batch:
                try:
                    edge_is_existing_vertex_pair = all(
                        endpoint in self._graph_key_ids for endpoint in argument
                    )
                except TypeError:
                    edge_is_existing_vertex_pair = False
                edges = (argument,) if edge_is_existing_vertex_pair else argument
            elif isinstance(argument, tuple) and len(argument) == 2:
                edges = (argument,)
            else:
                edges = argument
        edges = [tuple(edge) for edge in edges]
        if any(len(edge) != 2 for edge in edges):
            raise ValueError("Graph edges must contain two vertex keys")
        if positions is not None and not isinstance(positions, Mapping):
            raise TypeError("positions must be a mapping of vertex keys to positions")
        new_vertices, seen_new_vertices = [], set()
        try:
            for key in (key for edge in edges for key in edge):
                if key not in self._graph_key_ids and key not in seen_new_vertices:
                    seen_new_vertices.add(key)
                    new_vertices.append(key)
        except TypeError as error:
            raise TypeError("Graph vertex keys must be hashable") from error
        next_id = self._next_graph_id
        vertex_ids = list(range(next_id, next_id + len(new_vertices)))
        ids_by_key = dict(self._graph_key_ids)
        ids_by_key.update(zip(new_vertices, vertex_ids))
        edge_ids = [
            vertex_id
            for start, end in edges
            for vertex_id in (ids_by_key[start], ids_by_key[end])
        ]

        if new_vertices:
            center = _point(self.get_center())
            vertex_positions = [
                _point(positions.get(key, center) if positions is not None else center)
                for key in new_vertices
            ]
        else:
            vertex_positions = []

        global_vertex_config, keyed_vertex_config = _split_style_config(
            "vertex_config", vertex_config, _VERTEX_CONFIG_KEYS, set(new_vertices),
            per_key_supported={
                "color", "fill_color", "stroke_color", "fill_opacity",
                "stroke_width", "stroke_opacity", "radius",
            },
        )
        base_vertex_config = {**self._default_vertex_config, **global_vertex_config}
        vertex_style_overrides = _vertex_style_override_rows(
            new_vertices, keyed_vertex_config
        )

        global_edge_config, keyed_edge_config = _split_style_config(
            "edge_config", edge_config, _EDGE_CONFIG_KEYS, set(edges),
            per_key_supported=_EDGE_CONFIG_KEYS,
        )
        default_edge_config = _validated_key_style(
            "edge_config",
            {**self._default_edge_config, **global_edge_config},
            _EDGE_CONFIG_KEYS,
        )
        edge_style_overrides = _edge_style_override_rows(edges, keyed_edge_config)

        vertex_ids_js = _array(vertex_ids) if new_vertices else None
        vertex_positions_js = (
            _array(value for point in vertex_positions for value in point)
            if new_vertices else None
        )
        vertex_styles_js = _array(vertex_style_overrides) if new_vertices else None
        edge_ids_js = _array(edge_ids)
        edge_styles_js = _array(edge_style_overrides)

        vertex_options = (
            _graph_style_options(base_vertex_config, {}) if new_vertices else None
        )
        try:
            edge_options = _graph_style_options(self._default_vertex_config, default_edge_config)
        except BaseException:
            if vertex_options is not None:
                vertex_options.free()
            raise

        try:
            if new_vertices:
                transferred_vertex_options = vertex_options
                transferred_edge_options = edge_options
                vertex_options = None
                edge_options = None
                engine_call(
                    self._context().liveGraphAddVerticesAndEdgesWithOptions,
                    self._graph_handle,
                    vertex_ids_js,
                    vertex_positions_js,
                    vertex_styles_js,
                    transferred_vertex_options,
                    edge_ids_js,
                    edge_styles_js,
                    transferred_edge_options,
                    operation="Graph.add_edges",
                )
            else:
                transferred_edge_options = edge_options
                edge_options = None
                engine_call(
                    self._context().liveGraphAddEdgesWithOptions,
                    self._graph_handle,
                    edge_ids_js,
                    edge_styles_js,
                    transferred_edge_options,
                    operation="Graph.add_edges",
                )
        except BaseException:
            if vertex_options is not None:
                vertex_options.free()
            if edge_options is not None:
                edge_options.free()
            raise
        self._graph_key_ids.update(zip(new_vertices, vertex_ids))
        self._next_graph_id = next_id + len(new_vertices)
        self._cache_added_edges(edges)
        added_mobjects = [self[key] for key in new_vertices]
        added_mobjects.extend(self._edge_style_target(edge) for edge in edges)
        return _compat.VGroup(*added_mobjects)

    def remove_vertices(self, *vertices):
        """Remove vertices and return their former edges and vertices in a VGroup."""
        if len(vertices) == 1:
            try:
                hash(vertices[0])
            except TypeError:
                vertices = tuple(vertices[0])
        keys = list(vertices)
        if not keys:
            return _compat.VGroup()

        ids, removed_mobjects = [], []
        remaining_keys = set(self._graph_key_ids)
        remaining_edges = self._canonical_edge_keys()
        removed_edges = []
        for key in keys:
            try:
                vertex_id = self._graph_key_ids[key]
            except (KeyError, TypeError) as error:
                raise ValueError(
                    f"The graph does not contain a vertex with identifier '{key}'"
                ) from error
            if key not in remaining_keys:
                raise ValueError(
                    f"The graph does not contain a vertex with identifier '{key}'"
                )
            ids.append(vertex_id)
            incident_edges = [edge for edge in remaining_edges if key in edge]
            removed_mobjects.extend(self._edge_style_target(edge) for edge in incident_edges)
            removed_edges.extend(incident_edges)
            remaining_edges = [edge for edge in remaining_edges if edge not in incident_edges]
            removed_mobjects.append(self[key])
            remaining_keys.remove(key)

        engine_call(self._context().liveGraphRemoveVertices, self._graph_handle,
                    _array(ids), operation="Graph.remove_vertices")
        self._cache_removed_edges(removed_edges)
        labels_to_remove = [self._labels[key] for key in keys if key in self._labels]
        if labels_to_remove:
            self.remove(*labels_to_remove)
        for key in keys:
            del self._graph_key_ids[key]
            self._labels.pop(key, None)
            self._vertex_wrappers.pop(key, None)
        return _compat.VGroup(*removed_mobjects)

    def remove_edges(self, *edges):
        """Remove edges and return their former edge objects in a VGroup."""
        if len(edges) == 1:
            argument = edges[0]
            edge_batch = (
                isinstance(argument, tuple)
                and len(argument) == 2
                and all(isinstance(edge, (tuple, list)) and len(edge) == 2 for edge in argument)
            )
            if edge_batch:
                try:
                    edge_is_existing_vertex_pair = all(
                        endpoint in self._graph_key_ids for endpoint in argument
                    )
                except TypeError:
                    edge_is_existing_vertex_pair = False
                edges = (argument,) if edge_is_existing_vertex_pair else argument
            elif isinstance(argument, tuple) and len(argument) == 2:
                edges = (argument,)
            else:
                edges = argument
        edges = [tuple(edge) for edge in edges]
        if any(len(edge) != 2 for edge in edges):
            raise ValueError("Graph edges must contain two vertex keys")
        if not edges:
            return _compat.VGroup()
        removed_mobjects, seen = [], set()
        for edge in edges:
            try:
                mobject = self._edge_style_target(edge)
            except ValueError as error:
                raise ValueError(f"The graph does not contain a edge '{edge}'") from error
            if id(mobject) in seen:
                raise ValueError(f"The graph does not contain a edge '{edge}'")
            seen.add(id(mobject))
            removed_mobjects.append(mobject)
        engine_call(self._context().liveGraphRemoveEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.remove_edges")
        self._cache_removed_edges(edges)
        return _compat.VGroup(*removed_mobjects)

    def copy(self):
        """Copy the retained family, including appearance and endpoint bindings."""
        clone = object.__new__(type(self))
        clone._directed = self._directed
        clone._graph_key_ids = dict(self._graph_key_ids)
        clone._edge_attributes = {edge: dict(attributes) for edge, attributes in self._edge_attributes.items()}
        clone._edge_wrappers = {}
        clone._edge_mapping = None
        clone._edge_mapping_sources = {}
        clone._labels = {}
        clone.labels = clone._labels
        clone._vertex_wrappers = {}
        clone._next_graph_id = self._next_graph_id
        clone._default_vertex_config = dict(self._default_vertex_config)
        clone._default_edge_config = dict(self._default_edge_config)
        clone._canonical_live_target_context = self._context()
        clone._graph_handle = engine_call(
            clone._canonical_live_target_context.liveGraphCopy,
            self._graph_handle,
            operation="Graph.copy",
        )
        graph_family = engine_call(clone._graph_handle.family)
        graph_wrapper = object.__new__(_compat.Group)
        _shared._attach_shared_family(graph_wrapper, graph_family, self._context())
        graph_wrapper._canonical_live_target_context = self._context()
        clone._graph_family_wrapper = graph_wrapper
        graph_wrapper._refresh_semantic_members = clone._refresh_graph_family_members
        clone._labels = {key: label.copy() for key, label in self._labels.items()}
        clone.labels = clone._labels
        _compat.Group.__init__(clone, graph_wrapper, *clone._labels.values())
        return clone

    def change_layout(self, layout="spring", layout_scale=_LAYOUT_SCALE_UNSET, *,
                      scale=_LAYOUT_SCALE_UNSET,
                      center=(0.0, 0.0), positions=None, layout_config=None,
                      root_vertex=None, partitions=None):
        if layout_scale is not _LAYOUT_SCALE_UNSET and scale is not _LAYOUT_SCALE_UNSET:
            raise TypeError("change_layout accepts either layout_scale or scale, not both")
        scale = 2.0 if layout_scale is _LAYOUT_SCALE_UNSET and scale is _LAYOUT_SCALE_UNSET else (
            scale if layout_scale is _LAYOUT_SCALE_UNSET else layout_scale
        )
        if isinstance(layout, Mapping):
            positions, layout = layout, "explicit"
        if root_vertex is not None and layout != "tree":
            raise NotImplementedError("root_vertex is supported only by the tree layout")
        if partitions is not None and layout != "partite":
            raise NotImplementedError("partitions are supported only by the partite layout")
        if layout == "spring":
            (center, initial_positions, fixed_indices, spring_k,
             iterations, threshold, rescale, scale, scale_y, fixed_is_set) = _spring_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            edge_weights = self._layout_edge_weights(layout_config)
            engine_call(
                self._context().liveGraphSpringLayout,
                self._graph_handle,
                _array(edge_weights),
                _array(value for point in initial_positions for value in point),
                _array(fixed_indices), fixed_is_set,
                scale, scale_y, *center, spring_k, iterations, threshold, rescale,
                operation="Graph.spring_layout",
            )
        elif layout == "random":
            _, random_positions, scale, scale_y = _random_layout_config(
                layout_config, center, self._graph_key_ids, scale
            )
            engine_call(
                self._context().liveGraphRandomLayout,
                self._graph_handle,
                _array(value for point in random_positions for value in point),
                scale, scale_y,
                operation="Graph.random_layout",
            )
        elif layout == "tree":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            root_index, orientation_up, spacing, scale, scale_y = _tree_layout_config(
                layout_config, vertex_order, scale, self._directed,
                root_vertex=root_vertex,
            )
            spacing_x, spacing_y = (None, None) if spacing is None else spacing
            engine_call(
                self._context().liveGraphTreeLayout,
                self._graph_handle,
                root_index, orientation_up, spacing_x, spacing_y, scale, scale_y,
                operation="Graph.tree_layout",
            )
        elif layout == "circular":
            center = _circular_layout_center(layout_config, center)
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(self._context().liveGraphCircularLayout, self._graph_handle,
                        scale_x, scale_y, *_point(center), operation="Graph.circular_layout")
        elif layout == "shell":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            center, groups, rotate = _shell_layout_config(
                layout_config, center, vertex_order
            )
            if isinstance(scale, Sequence) and not isinstance(scale, (str, bytes)):
                raise NotImplementedError("shell layout_scale must be a scalar")
            shell_indices, shell_sizes = _flatten_groups(groups)
            engine_call(
                self._context().liveGraphShellLayout,
                self._graph_handle,
                _array(shell_indices), _array(shell_sizes),
                _shared._ir._finite_number("scale", scale), *center, rotate,
                operation="Graph.shell_layout",
            )
        elif layout == "spiral":
            center, resolution, equidistant = _spiral_layout_config(
                layout_config, center, len(self._graph_key_ids)
            )
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(
                self._context().liveGraphSpiralLayout,
                self._graph_handle,
                scale_x, scale_y, *center,
                resolution, equidistant,
                operation="Graph.spiral_layout",
            )
        elif layout == "partite":
            center, partitions, align_horizontal = _partite_layout_config(
                layout_config, center, self._graph_key_ids,
                partitions=partitions,
            )
            partition_indices, partition_sizes = _flatten_groups(partitions)
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(
                self._context().liveGraphPartiteLayout,
                self._graph_handle,
                _array(partition_indices), _array(partition_sizes),
                scale_x, scale_y, *center,
                align_horizontal,
                operation="Graph.partite_layout",
            )
        elif layout == "spectral":
            center, scale_x, scale_y = _spectral_layout_config(
                layout_config, center, scale,
            )
            edge_weights = self._layout_edge_weights(layout_config)
            engine_call(
                self._context().liveGraphSpectralLayout,
                self._graph_handle,
                _array(edge_weights),
                scale_x, scale_y, *center,
                operation="Graph.spectral_layout",
            )
        elif layout == "kamada_kawai":
            vertex_order = {key: index for index, key in enumerate(self._graph_key_ids)}
            center, distances, initial_positions = _kamada_kawai_layout_config(
                layout_config, center, vertex_order,
            )
            edge_weights = self._layout_edge_weights(
                layout_config, use_weight=not distances
            )
            scale_x, scale_y = _layout_scale_pair(scale)
            engine_call(
                self._context().liveGraphKamadaKawaiLayout,
                self._graph_handle,
                _array(edge_weights),
                _array(distances),
                _array(value for point in initial_positions for value in point),
                scale_x, scale_y, *center,
                operation="Graph.kamada_kawai_layout",
            )
        elif layout == "explicit":
            if not isinstance(positions, Mapping):
                raise TypeError("explicit Graph layout requires a key-to-position mapping")
            try:
                ordered = [_point(positions[key]) for key in self._graph_key_ids]
            except KeyError as error:
                raise ValueError("explicit Graph layout must cover every current vertex") from error
            if len(positions) != len(ordered):
                raise ValueError("explicit Graph layout must cover every current vertex exactly once")
            engine_call(self._context().liveGraphExplicitLayout, self._graph_handle,
                        _array(value for point in ordered for value in point),
                        operation="Graph.explicit_layout")
        else:
            raise NotImplementedError(
                "Python Graph qualifies only spring, random, tree, circular, shell, spiral, partite, spectral, kamada_kawai, and explicit layouts"
            )
        for key, label in self._labels.items():
            label.move_to(self._raw_vertex(key).get_center())
        return self


def _spring_layout_config(layout_config, fallback_center, vertex_ids, layout_scale):
    """Prepare NetworkX-compatible initial positions for Rust force integration."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "seed", "pos", "fixed", "k", "iterations", "threshold", "weight"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported spring layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("spring layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional spring layouts")
    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    count = len(vertex_ids)
    fixed_is_set = "fixed" in options and options["fixed"] is not None

    pos = options.get("pos")
    if pos is not None and not isinstance(pos, Mapping):
        raise TypeError("spring layout_config pos must map vertex keys to 2D positions")
    fixed = options.get("fixed")
    if fixed_is_set:
        if pos is None:
            raise ValueError("spring fixed vertices require a pos mapping")
        if isinstance(fixed, (str, bytes)):
            raise TypeError("spring layout_config fixed must be a sequence of vertex keys")
        try:
            fixed = list(fixed)
        except TypeError as error:
            raise TypeError("spring layout_config fixed must be a sequence of vertex keys") from error
        missing_positions = [key for key in fixed if key not in pos]
        if missing_positions:
            raise ValueError(f"spring fixed vertex {missing_positions[0]!r} is missing from pos")
    else:
        fixed = []

    raw_k = options.get("k")
    spring_k = None if raw_k is None else _shared._ir._finite_number("layout_config.k", raw_k)
    if spring_k == 0:
        raise ValueError("spring layout_config k must be non-zero")
    iterations = options.get("iterations", 50)
    if isinstance(iterations, bool) or not isinstance(iterations, Integral):
        raise TypeError("spring layout_config iterations must be an integer")
    if iterations < 0:
        raise ValueError("spring layout_config iterations must be non-negative")
    threshold = _shared._ir._finite_number(
        "layout_config.threshold", options.get("threshold", 1e-4)
    )
    if threshold < 0:
        raise ValueError("spring layout_config threshold must be non-negative")
    weight = options.get("weight", "weight")
    if weight is not None and not isinstance(weight, str):
        raise TypeError("spring layout_config weight must be a string or None")
    if layout_scale is None:
        scale, scale_y, rescale = 1.0, None, False
    else:
        scale, scale_y = _layout_scale_pair(layout_scale)
        rescale = not fixed_is_set

    np, rng = _numpy_random_source(options.get("seed"), "spring")

    def random_array(shape):
        return _numpy_random_array(rng, shape)

    provided = {}
    if pos is not None:
        try:
            domain_size = max(float(coordinate) for value in pos.values() for coordinate in value)
        except ValueError as error:
            raise ValueError("spring layout_config pos must contain at least one 2D position") from error
        if domain_size == 0:
            domain_size = 1.0
        for key in vertex_ids:
            if key in pos:
                point = _point(pos[key])
                provided[key] = point
        initial = random_array((count, 2)) * domain_size + np.asarray(center)
        for key, point in provided.items():
            initial[vertex_ids[key], :] = point
    else:
        domain_size = 1.0
        if count <= 1:
            initial = np.asarray([center] if count == 1 else [], dtype=float).reshape((count, 2))
        else:
            initial = random_array((count, 2))

    fixed_indices = [vertex_ids[key] for key in fixed if key in vertex_ids]

    if spring_k is None and fixed_is_set and count > 1:
        spring_k = domain_size / (count ** 0.5)

    initial_positions = [tuple(map(float, point)) for point in initial]
    return (
        center, initial_positions, fixed_indices, spring_k, int(iterations), threshold,
        rescale, scale, scale_y, fixed_is_set,
    )


def _spectral_layout_config(layout_config, fallback_center, layout_scale):
    """Validate the supported 2D spectral option subset for shared Rust."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "weight"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported spectral layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("spectral layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional spectral layouts")
    weight = options.get("weight", "weight")
    if weight is not None and not isinstance(weight, str):
        raise TypeError("spectral layout_config weight must be a string or None")
    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    scale_x, scale_y = _layout_scale_pair(layout_scale)
    return center, scale_x, scale_y


def _kamada_kawai_layout_config(layout_config, fallback_center, vertex_ids):
    """Flatten the supported 2D NetworkX Kamada-Kawai inputs for Rust."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "dist", "pos", "weight"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported Kamada-Kawai layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("Kamada-Kawai layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional Kamada-Kawai layouts")
    weight = options.get("weight", "weight")
    if weight is not None and not isinstance(weight, str):
        raise TypeError("Kamada-Kawai layout_config weight must be a string or None")

    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    distances = []
    if "dist" in options and options["dist"] is not None:
        raw_distances = options["dist"]
        if not isinstance(raw_distances, Mapping):
            raise TypeError("Kamada-Kawai layout_config dist must be a nested mapping")
        count = len(vertex_ids)
        matrix = [1e6] * (count * count)
        for source, row in raw_distances.items():
            if source not in vertex_ids:
                continue
            if not isinstance(row, Mapping):
                raise TypeError("Kamada-Kawai layout_config dist rows must be mappings")
            source_index = vertex_ids[source]
            for destination, distance in row.items():
                if destination not in vertex_ids:
                    continue
                distance = _shared._ir._finite_number("layout_config.dist", distance)
                if distance < 0 or (source != destination and distance == 0):
                    raise ValueError("Kamada-Kawai distances must be non-negative and positive between distinct vertices")
                matrix[source_index * count + vertex_ids[destination]] = distance
        distances = matrix

    positions = []
    if "pos" in options and options["pos"] is not None:
        raw_positions = options["pos"]
        if not isinstance(raw_positions, Mapping):
            raise TypeError("Kamada-Kawai layout_config pos must be a mapping")
        try:
            positions = [_point(raw_positions[key]) for key in vertex_ids]
        except KeyError as error:
            raise ValueError("Kamada-Kawai pos must provide every graph vertex") from error
    return center, distances, positions


def _random_layout_config(layout_config, fallback_center, vertex_ids, layout_scale):
    """Prepare Manim's scaled NetworkX random-layout coordinates."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"center", "dim", "seed"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported random layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )

    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, Integral):
        raise TypeError("random layout_config dim must be an integer")
    if dimension != 2:
        raise NotImplementedError("Graph currently supports only two-dimensional random layouts")
    raw_center = options.get("center", fallback_center)
    center = _point((0.0, 0.0) if raw_center is None else raw_center)
    scale, scale_y = _layout_scale_pair(layout_scale)

    np, rng = _numpy_random_source(options.get("seed"), "random")
    positions = (_numpy_random_array(rng, (len(vertex_ids), 2)) + np.asarray(center)).astype(np.float32)
    prepared = [tuple(map(float, point)) for point in positions]
    return center, prepared, scale, scale_y


def _tree_layout_config(layout_config, vertex_ids, layout_scale, directed, *, root_vertex=None):
    """Resolve Manim's tree-layout root, direction, and spacing options."""
    if directed:
        raise NotImplementedError("ManimCE tree layout requires an undirected Graph")
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    supported = {"root_vertex", "orientation", "vertex_spacing"}
    unknown = [key for key in options if key not in supported]
    if unknown:
        raise NotImplementedError(
            "unsupported tree layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    if root_vertex is not None and "root_vertex" not in options:
        options["root_vertex"] = root_vertex
    root = options.get("root_vertex")
    if root is None:
        raise ValueError("tree layout requires root_vertex")
    try:
        root_index = vertex_ids[root]
    except (KeyError, TypeError) as error:
        raise ValueError("tree root_vertex must be an existing graph vertex") from error
    orientation = options.get("orientation", "down")
    if not isinstance(orientation, str):
        raise TypeError("tree layout_config orientation must be 'down' or 'up'")
    if orientation not in {"down", "up"}:
        raise ValueError("tree layout_config orientation must be 'down' or 'up'")
    spacing = options.get("vertex_spacing")
    if spacing is not None:
        if isinstance(spacing, (str, bytes)) or not isinstance(spacing, Sequence) or len(spacing) != 2:
            raise TypeError("tree layout_config vertex_spacing must contain x and y values")
        spacing = (
            _shared._ir._finite_number("layout_config.vertex_spacing[0]", spacing[0]),
            _shared._ir._finite_number("layout_config.vertex_spacing[1]", spacing[1]),
        )
    scale, scale_y = _layout_scale_pair(layout_scale)
    return root_index, orientation == "up", spacing, scale, scale_y


def _numpy_random_source(seed, layout_name):
    try:
        import numpy as np
    except ImportError as error:  # pragma: no cover - only available in the browser runtime
        raise RuntimeError(f"{layout_name} Graph layout requires NumPy in the Manim runtime") from error
    if seed is None:
        rng = np.random
    elif seed is np.random:
        rng = seed
    elif isinstance(seed, Integral):
        rng = np.random.RandomState(int(seed))
    elif isinstance(seed, np.random.RandomState):
        rng = seed
    elif hasattr(np.random, "Generator") and isinstance(seed, np.random.Generator):
        rng = seed
    else:
        raise TypeError(
            f"{layout_name} layout_config seed must be an integer, RandomState, Generator, or None"
        )
    return np, rng


def _numpy_random_array(rng, shape):
    return rng.rand(*shape) if hasattr(rng, "rand") else rng.random(shape)


def _circular_layout_center(layout_config, fallback_center):
    """Resolve the supported NetworkX circular-layout configuration subset."""
    if layout_config is None:
        return _point(fallback_center)
    if not isinstance(layout_config, Mapping):
        raise TypeError("layout_config must be a mapping")
    options = dict(layout_config)
    unknown = [key for key in options if key not in {"center", "dim"}]
    if unknown:
        raise NotImplementedError(
            "unsupported circular layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("circular layout_config dim must be an integer")
    if dimension < 2:
        raise ValueError("circular layout_config dim must be at least 2")
    if dimension > 2:
        raise NotImplementedError("Graph currently supports only two-dimensional layouts")
    return _point(options.get("center", fallback_center))


def _shell_layout_config(layout_config, fallback_center, vertex_ids):
    """Resolve the qualified 2D NetworkX shell-layout option subset."""
    if layout_config is None:
        return _point(fallback_center), [], None
    if not isinstance(layout_config, Mapping):
        raise TypeError("layout_config must be a mapping")
    options = dict(layout_config)
    unknown = [key for key in options if key not in {"center", "dim", "nlist", "rotate"}]
    if unknown:
        raise NotImplementedError(
            "unsupported shell layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("shell layout_config dim must be an integer")
    if dimension != 2:
        raise ValueError("shell layout currently supports only dim=2")
    center = _point(options.get("center", fallback_center))
    # NetworkX returns the center directly for empty and singleton graphs,
    # before inspecting shell membership or rotation.
    if len(vertex_ids) <= 1:
        return center, [], None
    rotate = options.get("rotate")
    if rotate is not None:
        rotate = _shared._ir._finite_number("layout_config.rotate", rotate)
    nlist = options.get("nlist")
    groups = []
    if nlist is not None:
        if isinstance(nlist, (str, bytes)) or not isinstance(nlist, Sequence):
            raise TypeError("shell layout_config nlist must be a sequence of vertex sequences")
        if len(nlist) == 0 and vertex_ids:
            raise ValueError("shell layout_config nlist must not be empty")
        seen = set()
        for shell in nlist:
            if isinstance(shell, (str, bytes)) or not isinstance(shell, Sequence):
                raise TypeError("each shell in layout_config nlist must be a vertex sequence")
            group = []
            for key in shell:
                try:
                    vertex_id = vertex_ids[key]
                except (KeyError, TypeError) as error:
                    raise ValueError(f"shell layout references unknown vertex {key!r}") from error
                if vertex_id in seen:
                    raise ValueError("shell layout must assign each graph vertex exactly once")
                seen.add(vertex_id)
                group.append(vertex_id)
            groups.append(group)
        if len(seen) != len(vertex_ids):
            raise ValueError("shell layout must assign each graph vertex exactly once")
    return center, groups, rotate


def _flatten_groups(groups):
    return [index for group in groups for index in group], [len(group) for group in groups]


def _spiral_layout_config(layout_config, fallback_center, vertex_count):
    """Resolve the qualified 2D NetworkX spiral-layout option subset."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    unknown = [key for key in options if key not in {"center", "dim", "resolution", "equidistant"}]
    if unknown:
        raise NotImplementedError(
            "unsupported spiral layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    dimension = options.get("dim", 2)
    if isinstance(dimension, bool) or not isinstance(dimension, int):
        raise TypeError("spiral layout_config dim must be an integer")
    if dimension != 2:
        raise ValueError("spiral layout currently supports only dim=2")
    center = _point(options.get("center", fallback_center))
    # NetworkX returns immediately for empty and singleton graphs, so layout
    # tuning values are intentionally not inspected in those cases.
    if vertex_count <= 1:
        return center, 0.35, False
    resolution = _shared._ir._finite_number(
        "layout_config.resolution", options.get("resolution", 0.35)
    )
    equidistant = options.get("equidistant", False)
    if not isinstance(equidistant, bool):
        raise TypeError("spiral layout_config equidistant must be a bool")
    return center, resolution, equidistant


def _partite_layout_config(layout_config, fallback_center, vertex_ids, *, partitions=None):
    """Resolve Manim's partition assignment and qualified NetworkX align options."""
    if layout_config is None:
        options = {}
    elif isinstance(layout_config, Mapping):
        options = dict(layout_config)
    else:
        raise TypeError("layout_config must be a mapping")
    unknown = [key for key in options if key not in {"partitions", "align", "center"}]
    if unknown:
        raise NotImplementedError(
            "unsupported partite layout_config option(s): "
            + ", ".join(sorted(map(str, unknown)))
        )
    center = _point(options.get("center", fallback_center))
    align = options.get("align", "vertical")
    if align not in ("vertical", "horizontal"):
        raise ValueError("partite layout_config align must be 'vertical' or 'horizontal'")
    specifications = options.get("partitions", partitions)
    if specifications is None or isinstance(specifications, (str, bytes)) or not isinstance(specifications, Sequence):
        raise ValueError("partite layout requires a non-empty partitions sequence")
    if not specifications:
        raise ValueError("partite layout requires a non-empty partitions sequence")

    position_by_key = {key: index for index, key in enumerate(vertex_ids)}
    layer_by_position = {}
    for layer, partition in enumerate(specifications):
        if isinstance(partition, (str, bytes)) or not isinstance(partition, Sequence):
            raise TypeError("each partite partition must be a sequence of vertex keys")
        for key in partition:
            try:
                position = position_by_key[key]
            except (KeyError, TypeError) as error:
                raise ValueError(f"partite layout references unknown vertex {key!r}") from error
            # Manim assigns each partition in order, so a repeated key receives
            # the subset number from its last occurrence.
            layer_by_position[position] = layer
    for position in range(len(position_by_key)):
        layer_by_position.setdefault(position, len(specifications))

    ordered_layers = sorted(set(layer_by_position.values()))
    groups = [
        [position for position in range(len(position_by_key))
         if layer_by_position[position] == layer]
        for layer in ordered_layers
    ]
    return center, groups, align == "horizontal"


class Graph(_GraphBase):
    """Undirected retained Graph with qualified spring, random, tree, circular, shell, spiral, partite, and explicit layouts."""


class DiGraph(_GraphBase):
    """Directed retained Graph with shared Rust spring/random layouts and Arrow semantics."""
    _directed = True


__all__ = ["Graph", "DiGraph"]
