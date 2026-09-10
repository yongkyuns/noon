"""Argument/result adaptation for immutable shared Rust path observations."""
import operator
import noon as _base
import _noon_ir as _ir
from _noon_errors import engine_call
from _manim_semantic_handles import _typed_manim_observation


def _query(value):
    query = _typed_manim_observation(value, "pathQuery", "queryMobjectPath")
    if query is None:
        raise RuntimeError("path queries require the shared Rust authoring host")
    return query


def point_from_proportion(value, alpha):
    alpha = _ir._finite_number("alpha", alpha)
    query = _query(value)
    try:
        coordinates = engine_call(query.pointFromProportion, alpha)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
    finally:
        query.free()


def arc_length(value, sample_points_per_curve):
    samples = None if sample_points_per_curve is None else operator.index(sample_points_per_curve)
    if samples is not None and not 2 <= samples <= 0xFFFFFFFF:
        raise ValueError("sample_points_per_curve must be between 2 and 4294967295")
    query = _query(value)
    try:
        return float(engine_call(query.arcLength, samples))
    finally:
        query.free()


def endpoint(value, end):
    query = _query(value)
    try:
        coordinates = engine_call(query.end if end else query.start)
        return _base.Vec2(float(coordinates[0]), float(coordinates[1]))
    finally:
        query.free()
