"""Thin Graph/DiGraph adapters over the shared Rust authoring handle.

Python owns only its arbitrary hashable-key to u32 mapping. Rust owns graph
topology, arrow endpoints, persistent placements, and publication.
"""

from __future__ import annotations

from collections.abc import Hashable, Mapping

import _manim_compat as _compat
import _manim_semantic_handles as _shared
import noon as _base
from _noon_errors import engine_call

try:
    from pyodide.ffi import to_js as _to_js
except ImportError:  # pragma: no cover - native import smoke only
    _to_js = None


def _array(values):
    if _to_js is None:
        raise RuntimeError("Graph requires the shared Rust authoring host")
    return _to_js(list(values))


def _point(value):
    point = _base._as_vec2(value)
    return float(point.x), float(point.y)


class _GraphBase(_compat.Group):
    _directed = False

    def __init__(self, vertices, edges, layout="circular", *, layout_scale=2.0,
                 layout_center=(0.0, 0.0), **kwargs):
        if kwargs:
            raise TypeError("unsupported Graph option(s): " + ", ".join(sorted(kwargs)))
        explicit = layout if isinstance(layout, Mapping) else (
            vertices if layout in {None, "explicit"} and isinstance(vertices, Mapping) else None
        )
        circular = isinstance(layout, str) and layout == "circular"
        if explicit is None and not circular:
            raise NotImplementedError("Graph requires a circular layout or a complete position mapping")
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
        center = _point(layout_center)
        scale = _shared._ir._finite_number("layout_scale", layout_scale)
        context = _shared._live_constructor_context("Graph", allow_unstarted=True)
        if context is None:
            raise RuntimeError("Graph construction requires a canonical Scene authoring context")
        ids = _array(self._graph_key_ids.values())
        if circular:
            self._graph_handle = engine_call(
                context.liveCreateCircularGraph, bool(self._directed), ids, _array(edge_ids),
                scale, *center, operation="Graph.create",
            )
        else:
            self._graph_handle = engine_call(
                context.liveCreateGraph, bool(self._directed), ids, _array(positions),
                _array(edge_ids), operation="Graph.create",
            )
        _shared._attach_shared_family(self, engine_call(self._graph_handle.family), context)
        self._canonical_live_target_context = context

    def _edge_ids(self, edges):
        values = []
        for start, end in edges:
            try:
                values.extend((self._graph_key_ids[start], self._graph_key_ids[end]))
            except KeyError as error:
                raise ValueError("Graph edge references an unknown vertex key") from error
        return values

    def _refresh_semantic_members(self):
        _shared._attach_shared_family(self, self._semantic_family_handle, self._context())

    def _context(self):
        return self._canonical_live_target_context

    def __getitem__(self, key):
        """Resolve a vertex key to its existing shared semantic Mobject."""
        handle = engine_call(self._graph_handle.vertex, self._graph_key_ids[key])
        if handle is None:
            raise KeyError(key)
        identity = f"{int(handle.semanticSlot)}:{int(handle.semanticGeneration)}"
        member = self._semantic_member_wrappers.get(identity)
        if member is None:
            member = object.__new__(_compat.VMobject)
            _shared._attach_shared_handle(member, handle)
            member._canonical_live_target_context = self._context()
            self._semantic_member_wrappers[identity] = member
        return member

    @property
    def vertices(self):
        return {key: self[key] for key in self._graph_key_ids}

    def add_vertices(self, vertices):
        if not isinstance(vertices, Mapping):
            raise TypeError("Graph vertices must be a mapping of hashable keys to positions")
        pending, ids, positions = [], [], []
        next_id = self._next_graph_id
        for key, position in vertices.items():
            if not isinstance(key, Hashable):
                raise TypeError("Graph vertex keys must be hashable")
            if key in self._graph_key_ids or any(key == item[0] for item in pending):
                raise ValueError("duplicate Graph vertex key")
            x, y = _point(position)
            pending.append((key, x, y))
            ids.append(next_id)
            next_id += 1
            positions.extend((x, y))
        engine_call(self._context().liveGraphAddVertices, self._graph_handle,
                    _array(ids), _array(positions), operation="Graph.add_vertices")
        self._graph_key_ids.update((key, vertex_id) for (key, _, _), vertex_id in zip(pending, ids))
        self._next_graph_id = next_id
        return self

    def add_edges(self, edges):
        """Add only edges whose endpoints already exist.

        Manim's implicit missing-vertex creation needs a shared compound
        publication with default placement; it is deliberately unsupported
        here so Python never splits that operation into transactions.
        """
        engine_call(self._context().liveGraphAddEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.add_edges")
        return self

    def remove_vertices(self, vertices):
        keys = list(vertices)
        try:
            ids = [self._graph_key_ids[key] for key in keys]
        except KeyError as error:
            raise ValueError("unknown Graph vertex key") from error
        engine_call(self._context().liveGraphRemoveVertices, self._graph_handle,
                    _array(ids), operation="Graph.remove_vertices")
        for key in keys:
            del self._graph_key_ids[key]
        return self

    def remove_edges(self, edges):
        engine_call(self._context().liveGraphRemoveEdges, self._graph_handle,
                    _array(self._edge_ids(edges)), operation="Graph.remove_edges")
        return self

    def copy(self):
        """Copy the retained family, including appearance and endpoint bindings."""
        clone = object.__new__(type(self))
        clone._directed = self._directed
        clone._graph_key_ids = dict(self._graph_key_ids)
        clone._next_graph_id = self._next_graph_id
        clone._canonical_live_target_context = self._context()
        clone._graph_handle = engine_call(
            clone._canonical_live_target_context.liveGraphCopy,
            self._graph_handle,
            operation="Graph.copy",
        )
        _shared._attach_shared_family(clone, engine_call(clone._graph_handle.family), self._context())
        return clone

    def change_layout(self, layout="circular", *, scale=2.0, center=(0.0, 0.0), positions=None):
        if isinstance(layout, Mapping):
            positions, layout = layout, "explicit"
        if layout == "circular":
            engine_call(self._context().liveGraphCircularLayout, self._graph_handle,
                        float(scale), *_point(center), operation="Graph.circular_layout")
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
            raise NotImplementedError("Python Graph qualifies only circular and explicit layouts")
        return self


class Graph(_GraphBase):
    """Undirected retained Graph with qualified circular/explicit layouts."""


class DiGraph(_GraphBase):
    """Directed retained Graph using Rust's shared Arrow endpoint semantics."""
    _directed = True


__all__ = ["Graph", "DiGraph"]
