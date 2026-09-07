"""Test-only typed geometry bridge used by CPython facade fixtures."""

from __future__ import annotations

import copy
import json


class FakeVectorPath:
    def __init__(self) -> None:
        self.commands = []

    def moveTo(self, x, y):
        self.commands.append({"move_to": {"to": {"x": x, "y": y}}})

    def lineTo(self, x, y):
        self.commands.append({"line_to": {"to": {"x": x, "y": y}}})

    def quadraticTo(self, cx, cy, x, y):
        self.commands.append({
            "quadratic_to": {
                "control": {"x": cx, "y": cy},
                "to": {"x": x, "y": y},
            }
        })

    def cubicTo(self, c1x, c1y, c2x, c2y, x, y):
        self.commands.append({
            "cubic_to": {
                "control1": {"x": c1x, "y": c1y},
                "control2": {"x": c2x, "y": c2y},
                "to": {"x": x, "y": y},
            }
        })

    def close(self):
        self.commands.append("close")


class FakeGeometryOptions:
    def __init__(self, geometry) -> None:
        self.snapshot = {
            "geometry": copy.deepcopy(geometry),
            "transform": {
                "translation": {"x": 0.0, "y": 0.0},
                "rotation": 0.0,
                "scale": {"x": 1.0, "y": 1.0},
            },
            "style": {
                "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 0.0},
                "stroke": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
                "stroke_width": 0.04,
                "stroke_width_mode": "screen_space",
                "stroke_join": "miter",
                "stroke_cap": "butt",
                "opacity": 1.0,
            },
        }

    @staticmethod
    def circle(radius):
        value = FakeGeometryOptions({"circle": {"radius": radius}})
        red = 0xFC / 255
        green = 0x62 / 255
        blue = 0x55 / 255
        value.snapshot["style"]["fill"].update(red=red, green=green, blue=blue)
        value.snapshot["style"]["stroke"].update(red=red, green=green, blue=blue)
        return value

    @staticmethod
    def square(side):
        return FakeGeometryOptions.rectangle(side, side)

    @staticmethod
    def rectangle(width, height):
        return FakeGeometryOptions({"rectangle": {"size": {"x": width, "y": height}}})

    @staticmethod
    def line(x1, y1, x2, y2):
        value = FakeGeometryOptions({
            "line": {"start": {"x": x1, "y": y1}, "end": {"x": x2, "y": y2}}
        })
        value.disableFill()
        return value

    @staticmethod
    def path(path):
        return FakeGeometryOptions({"vector_path": {"commands": list(path.commands)}})

    def setTranslation(self, x, y):
        self.snapshot["transform"]["translation"] = {"x": x, "y": y}

    def setRotation(self, angle):
        self.snapshot["transform"]["rotation"] = angle

    def setScale(self, x, y):
        self.snapshot["transform"]["scale"] = {"x": x, "y": y}

    def setColor(self, red, green, blue, alpha):
        del alpha
        if self.snapshot["style"]["fill"] is not None:
            self.snapshot["style"]["fill"].update(red=red, green=green, blue=blue)
        if self.snapshot["style"]["stroke"] is not None:
            self.snapshot["style"]["stroke"].update(red=red, green=green, blue=blue)

    def disableFill(self):
        self.snapshot["style"]["fill"] = None

    def setFill(self, red, green, blue, alpha):
        self.snapshot["style"]["fill"] = {
            "red": red, "green": green, "blue": blue, "alpha": alpha
        }

    setFillColor = setFill

    def setFillOpacity(self, opacity):
        if self.snapshot["style"]["fill"] is not None:
            self.snapshot["style"]["fill"]["alpha"] = opacity

    def disableStroke(self):
        self.snapshot["style"]["stroke"] = None

    def setStroke(self, red, green, blue, alpha):
        self.snapshot["style"]["stroke"] = {
            "red": red, "green": green, "blue": blue, "alpha": alpha
        }

    setStrokeColor = setStroke

    def setStrokeOpacity(self, opacity):
        if self.snapshot["style"]["stroke"] is not None:
            self.snapshot["style"]["stroke"]["alpha"] = opacity

    def setStrokeWidth(self, width):
        self.snapshot["style"]["stroke_width"] = width

    def setStrokeWidthMode(self, mode):
        self.snapshot["style"]["stroke_width_mode"] = mode

    def setStrokeJoin(self, join):
        self.snapshot["style"]["stroke_join"] = join

    def setStrokeCap(self, cap):
        self.snapshot["style"]["stroke_cap"] = cap

    def setObjectOpacity(self, opacity):
        self.snapshot["style"]["opacity"] = opacity


def install_js_bridge(fake_js, create_from_snapshot_json) -> None:
    fake_js.noonAuthoringGeometryOptions = FakeGeometryOptions
    fake_js.noonAuthoringVectorPath = FakeVectorPath
    fake_js.noonCreateAuthoringGeometryHandle = (
        lambda options: create_from_snapshot_json(json.dumps(options.snapshot))
    )


def install_module_bridge(module, create_from_snapshot_json) -> None:
    module._geometry_options = FakeGeometryOptions
    module._authoring_vector_path = FakeVectorPath
    module._create_geometry_handle = (
        lambda options: create_from_snapshot_json(json.dumps(options.snapshot))
    )


def _options_from_result(result) -> FakeGeometryOptions:
    value = FakeGeometryOptions(result.snapshot["geometry"])
    value.snapshot = copy.deepcopy(result.snapshot)
    return value


def install_option_factory(fake_js, name, factory) -> None:
    """Adapt an existing geometry-producing fake to the inert options boundary."""
    setattr(
        fake_js.noonAuthoringGeometryOptions,
        name,
        staticmethod(lambda *args: _options_from_result(factory(*args))),
    )
