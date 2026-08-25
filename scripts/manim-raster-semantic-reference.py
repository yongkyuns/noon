#!/usr/bin/env python3
"""Capture renderer-independent Manim scene state at every canonical raster frame.

This intentionally runs Manim's real Scene/Cairo animation loop while replacing
pixel/file output with a no-op renderer sink.  The resulting frame indices match
Manim's rendered frame progression without duplicating the expensive Cairo raster
work already performed by the differential oracle.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import math
from pathlib import Path
from types import SimpleNamespace
from typing import Any

import numpy as np
from manim import config, tempconfig
from manim.renderer.cairo_renderer import CairoRenderer

PINNED_MANIM_VERSION = "0.21.0"
CAIRO_STROKE_WIDTH_SCALE = 0.01


class NullFileWriter:
    """Minimum SceneFileWriter surface used by CairoRenderer.play()."""

    def __init__(self, _renderer: CairoRenderer, _scene_name: str) -> None:
        self.sections = [SimpleNamespace(skip_animations=False)]

    def add_partial_movie_file(self, _hash: str | None) -> None:
        pass

    def begin_animation(self, _allow_write: bool) -> None:
        pass

    def end_animation(self, _allow_write: bool) -> None:
        pass


class SemanticRenderer(CairoRenderer):
    def __init__(self) -> None:
        self.frames: list[dict[str, Any]] = []
        self._active_scene = None
        super().__init__(file_writer_class=NullFileWriter)

    def play(self, scene, *args, **kwargs) -> None:  # type: ignore[no-untyped-def]
        self._active_scene = scene
        super().play(scene, *args, **kwargs)

    def save_static_frame_data(self, _scene, _static_mobjects):  # type: ignore[no-untyped-def]
        self.static_image = None
        return None

    def update_frame(self, _scene, *args, **kwargs) -> None:  # type: ignore[no-untyped-def]
        # Semantic capture does not need a camera pixel buffer.
        return None

    def render(self, scene, time, moving_mobjects=None) -> None:  # type: ignore[no-untyped-def]
        del moving_mobjects
        self.frames.append(_scene_state(scene, len(self.frames), self.time, float(time)))
        self.time += 1.0 / float(config.frame_rate)

    def freeze_current_frame(self, duration: float) -> None:
        if self._active_scene is None:
            raise RuntimeError("semantic renderer has no active scene for frozen frame")
        dt = 1.0 / float(config.frame_rate)
        for _ in range(int(duration / dt)):
            self.frames.append(
                _scene_state(self._active_scene, len(self.frames), self.time, 0.0)
            )
            self.time += dt


def _scalar(value: Any) -> float:
    array = np.asarray(value, dtype=float).reshape(-1)
    if array.size == 0:
        return 0.0
    return float(array[0])


def _rgb(color: Any) -> list[float] | None:
    if color is None:
        return None
    values = np.asarray(color.to_rgb(), dtype=float).reshape(-1)
    if values.size < 3:
        return None
    return [float(values[0]), float(values[1]), float(values[2])]


def _rgba(mobject: Any, kind: str) -> dict[str, float] | None:
    color_getter = getattr(mobject, f"get_{kind}_color", None)
    opacity_getter = getattr(mobject, f"get_{kind}_opacity", None)
    if color_getter is None or opacity_getter is None:
        return None
    rgb = _rgb(color_getter())
    if rgb is None:
        return None
    return {
        "red": rgb[0],
        "green": rgb[1],
        "blue": rgb[2],
        "alpha": _scalar(opacity_getter()),
    }


def _object_state(mobject: Any, index: int) -> dict[str, Any]:
    center_array = np.asarray(mobject.get_center(), dtype=float).reshape(-1)
    center = [float(center_array[0]), float(center_array[1])]
    width = float(mobject.width)
    height = float(mobject.height)
    half_width = width * 0.5
    half_height = height * 0.5
    stroke_width_getter = getattr(mobject, "get_stroke_width", None)
    stroke_width = (
        _scalar(stroke_width_getter()) * CAIRO_STROKE_WIDTH_SCALE
        if stroke_width_getter is not None
        else 0.0
    )
    family_getter = getattr(mobject, "get_family", None)
    family_count = len(family_getter()) if family_getter is not None else 1
    return {
        "index": index,
        "type": type(mobject).__name__,
        "center": center,
        "bounds": {
            "min": [center[0] - half_width, center[1] - half_height],
            "max": [center[0] + half_width, center[1] + half_height],
            "width": width,
            "height": height,
        },
        "fill": _rgba(mobject, "fill"),
        "stroke": _rgba(mobject, "stroke"),
        "stroke_width": stroke_width,
        "family_count": family_count,
    }


def _scene_state(scene: Any, frame_index: int, scene_time: float, animation_time: float) -> dict[str, Any]:
    objects = [_object_state(mobject, index) for index, mobject in enumerate(scene.mobjects)]
    return {
        "engine": "manim",
        "frame_index": frame_index,
        "time": scene_time,
        "animation_time": animation_time,
        "present_object_count": len(objects),
        "objects": objects,
    }


def _load_source(source_path: Path):
    spec = importlib.util.spec_from_file_location("_noon_manim_raster_source", source_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load canonical source {source_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _render_fixture(module: Any, fixture: dict[str, Any], frame_rate: float) -> dict[str, Any]:
    scene_class = getattr(module, fixture["scene"])
    renderer = SemanticRenderer()
    scene = scene_class(renderer=renderer)
    scene.setup()
    try:
        scene.construct()
    finally:
        scene.tear_down()

    expected_frames = int(round(float(fixture["expected_duration"]) * frame_rate))
    if len(renderer.frames) != expected_frames:
        raise RuntimeError(
            f"{fixture['id']}: semantic frame count {len(renderer.frames)} != "
            f"expected {expected_frames} from duration/fps"
        )
    for index, frame in enumerate(renderer.frames):
        expected_time = index / frame_rate
        if not math.isclose(float(frame["time"]), expected_time, rel_tol=0.0, abs_tol=1e-9):
            raise RuntimeError(
                f"{fixture['id']}: semantic frame {index} time {frame['time']} != {expected_time}"
            )
    return {
        "id": fixture["id"],
        "scene": fixture["scene"],
        "frame_count": len(renderer.frames),
        "frames": renderer.frames,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    import manim

    if manim.__version__ != PINNED_MANIM_VERSION:
        raise SystemExit(
            f"expected ManimCE {PINNED_MANIM_VERSION}, found {manim.__version__}"
        )

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    reference = manifest["reference"]
    source_path = (args.manifest.parent.parent.parent / reference["source"]).resolve()
    settings = {
        "renderer": "cairo",
        "frame_rate": float(reference["frame_rate"]),
        "pixel_width": int(reference["pixel_width"]),
        "pixel_height": int(reference["pixel_height"]),
        "progress_bar": "none",
        "disable_caching": True,
        "save_last_frame": False,
        "write_to_movie": False,
    }
    with tempconfig(settings):
        module = _load_source(source_path)
        fixtures = [
            _render_fixture(module, fixture, float(reference["frame_rate"]))
            for fixture in manifest["fixtures"]
        ]

    payload = {
        "manim_version": manim.__version__,
        "frame_rate": float(reference["frame_rate"]),
        "fixtures": fixtures,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Captured semantic state for {len(fixtures)} Manim raster fixtures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
