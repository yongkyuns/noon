#!/usr/bin/env python3
"""Actual ManimCE/Cairo pixels and observed tip geometry, never a drawn surrogate."""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path

import manim
from manim import Arrow, tempconfig
from manim.renderer.cairo_renderer import CairoRenderer
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "parity/manim-v0.21/core-examples/arrow_tip_regression.py"
VERSION = "0.21.0"
EXPECTED_CASES = 16
EXPECTED_TIPS = 18
EXPECTED_DURATION = 1.0


def load_module(name, filename):
    spec = importlib.util.spec_from_file_location(name, filename)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {filename}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


# Reuse the existing oracle's logical play/wait clock. Cairo still rasterizes
# every captured frame; only file output and read-only observations are replaced.
clock = load_module("arrow_reference_clock", ROOT / "scripts/manim-raster-semantic-reference.py")


class TipRenderer(clock.SemanticRenderer):
    def __init__(self, directory, dpr):
        super().__init__()
        self.directory = directory
        self.dpr = dpr
        self.captures = []

    def capture(self, scene, time):
        CairoRenderer.update_frame(self, scene, ignore_skipping=True)
        image = Image.fromarray(self.get_frame()).convert("RGBA")
        filename = f"frame-{len(self.captures):04d}.png"
        image.save(self.directory / filename)
        width, height = image.size
        rois = []
        for name, arrow in scene.arrow_tip_cases:
            tips = [("end", arrow.get_tip())]
            if arrow.has_start_tip():
                tips.append(("start", arrow.start_tip))
            for side, tip in tips:
                points = [[float(p[0]), float(p[1])] for p in tip.get_start_anchors()]
                if len(points) != 3 or not all(math.isfinite(v) for p in points for v in p):
                    raise AssertionError(f"{name}/{side}: expected a finite triangle")
                px = [(p[0] / self.camera.frame_width + 0.5) * width for p in points]
                py = [(0.5 - p[1] / self.camera.frame_height) * height for p in points]
                margin = 3 * self.dpr
                x, y = math.floor(min(px)) - margin, math.floor(min(py)) - margin
                right, bottom = math.ceil(max(px)) + margin, math.ceil(max(py)) + margin
                if not (0 <= x < right <= width and 0 <= y < bottom <= height):
                    raise AssertionError(f"{name}/{side}: tip crop leaves the viewport")
                rois.append(dict(name=f"{name}-{side}", points=points,
                                 x=x, y=y, width=right-x, height=bottom-y))
        if len(rois) != 18:
            raise AssertionError(f"expected all 18 tips, found {len(rois)}")
        self.captures.append(dict(time=float(time), image=filename, rois=rois))

    def render(self, scene, time, moving_mobjects=None):
        self.capture(scene, self.logical_time + float(time))
        super().render(scene, time, moving_mobjects)

    def freeze_current_frame(self, duration):
        self.capture(self._active_scene, self.logical_time)
        super().freeze_current_frame(duration)


def assert_fixture_motion_contract(module, renderer):
    if len(module.ARROW_TIP_CASES) != EXPECTED_CASES:
        raise AssertionError(f"expected {EXPECTED_CASES} arrow cases")
    if sum(1 + (kind == "double") for _, kind, *_ in module.ARROW_TIP_CASES) != EXPECTED_TIPS:
        raise AssertionError(f"expected {EXPECTED_TIPS} tips")
    if tuple(module.MOTION_SHIFT) != (0.03, 0.018, 0):
        raise AssertionError("unexpected shared arrow-tip displacement")
    if not math.isclose(module.MOTION_DURATION, 0.5, abs_tol=1e-9):
        raise AssertionError("unexpected shared arrow-tip motion duration")
    if not math.isclose(renderer.logical_time, EXPECTED_DURATION, abs_tol=1e-9):
        raise AssertionError(f"unexpected duration {renderer.logical_time}")
    if not renderer.captures or renderer.captures[0]["time"] != 0.0:
        raise AssertionError("missing arrow-tip baseline capture")
    # The final capture is the start of the last quiet wait, immediately after
    # the common motion completes. Compare every observed tip apex, rather than
    # accepting a fixture representation that merely has the same total time.
    baseline = {roi["name"]: roi["points"][0] for roi in renderer.captures[0]["rois"]}
    completed = {roi["name"]: roi["points"][0] for roi in renderer.captures[-1]["rois"]}
    if baseline.keys() != completed.keys() or len(baseline) != EXPECTED_TIPS:
        raise AssertionError("arrow-tip motion changed the observed tip set")
    for name, before in baseline.items():
        after = completed[name]
        if not (
            math.isclose(after[0] - before[0], module.MOTION_SHIFT[0], abs_tol=1e-9)
            and math.isclose(after[1] - before[1], module.MOTION_SHIFT[1], abs_tol=1e-9)
        ):
            raise AssertionError(f"{name}: shared motion changed")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, default=ROOT / "ci-artifacts/arrow-tip")
    args = parser.parse_args()
    if manim.__version__ != VERSION:
        raise RuntimeError(f"expected ManimCE {VERSION}, got {manim.__version__}")
    args.output.mkdir(parents=True, exist_ok=True)
    report = dict(manim_version=VERSION, renderer="cairo",
                  source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(), runs=[])
    for dpr in (1, 2):
        directory = args.output / f"reference-dpr{dpr}"
        directory.mkdir(parents=True, exist_ok=True)
        settings = dict(renderer="cairo", pixel_width=960*dpr, pixel_height=540*dpr,
                        frame_rate=24, frame_height=8, frame_width=128/9,
                        background_color="#000000", background_opacity=1,
                        progress_bar="none", disable_caching=True,
                        save_last_frame=False, write_to_movie=False)
        with tempconfig(settings):
            module = load_module(f"arrow_tip_fixture_{dpr}", SOURCE)
            renderer = TipRenderer(directory, dpr)
            scene = module.ArrowTipRegression(renderer=renderer)
            scene.setup()
            try:
                scene.construct()
            finally:
                scene.tear_down()
            assert_fixture_motion_contract(module, renderer)
            if len(renderer.frames) != len(renderer.captures) or len(renderer.captures) < 10:
                raise AssertionError("incomplete reference frame sequence")
            count = len(renderer.captures)
            samples = sorted({0, 1, 2, count//2, count-2, count-1})
            report["runs"].append(dict(dpr=dpr, width=960*dpr, height=540*dpr,
                                      duration=1.0, samples=samples, frames=renderer.captures,
                                      directory=directory.name))
            # Demonstrate Manim's actual width API separately; this unsupported
            # Noon surface is deliberately not smuggled into the shared fixture.
            custom = Arrow((-1, 0, 0), (1, 0, 0), buff=0, tip_length=0.3,
                           tip_style={"width": 0.16})
            points = custom.tip.get_start_anchors()
            base_width = float(math.dist(points[1], points[2]))
            if not math.isclose(base_width, 0.16, abs_tol=1e-9):
                raise AssertionError("Manim independent tip width changed")
            report["manim_custom_width"] = dict(length=float(custom.tip.length), width=base_width,
                                                 api="tip_style={'width': 0.16}")
    (args.output / "reference.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"ManimCE {VERSION}: {len(report['runs'])} DPR runs, 18 tips per frame")


if __name__ == "__main__":
    main()
