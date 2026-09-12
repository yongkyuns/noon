import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimBraceFacadeTests(unittest.TestCase):
    def _run_source(self, source: str) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            value for value in (str(python_dir), env.get("PYTHONPATH")) if value
        )
        completed = subprocess.run(
            [sys.executable, "-c", textwrap.dedent(source)],
            cwd=python_dir,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            completed.returncode,
            0,
            f"stdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )

    def test_brace_facade_stays_thin_over_shared_rust_geometry(self) -> None:
        self._run_source(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            calls = []

            import _typed_geometry_test_support as support

            def path_options(tag):
                value = support.FakeGeometryOptions({
                    "vector_path": {
                        "commands": [
                            {"move_to": {"to": {"x": -1.0, "y": -1.0}}},
                            {"cubic_to": {
                                "control1": {"x": -0.5, "y": -1.2},
                                "control2": {"x": 0.5, "y": -1.2},
                                "to": {"x": 1.0, "y": -1.0},
                            }},
                        ],
                        "tag": tag,
                    }
                })
                value.setStrokeWidth(0.0)
                value.setFillOpacity(1.0)
                return value

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)
                def beginBrace(self, direction_x, direction_y, buff, sharpness):
                    calls.append((
                        "brace", float(direction_x), float(direction_y),
                        float(buff), float(sharpness),
                    ))
                    return path_options("brace")

            def create_from_snapshot(value):
                return FakeHandle(json.loads(value))

            support.install_js_bridge(fake_js, create_from_snapshot)

            def brace_between_points(x1, y1, x2, y2, dx, dy, buff, sharpness):
                calls.append((
                    "between", float(x1), float(y1), float(x2), float(y2),
                    float(dx), float(dy), float(buff), float(sharpness),
                ))
                return path_options("between")

            support.install_option_factory(
                fake_js, "braceBetweenPoints", brace_between_points
            )
            sys.modules["js"] = fake_js

            import noon
            from noon import Brace, BraceBetweenPoints, Square

            target = Square(2.0)
            brace = Brace(
                target,
                direction=noon.DOWN,
                buff=0.3,
                sharpness=1.5,
                color="#58C4DD",
            )
            between = BraceBetweenPoints(
                (-2.0, 1.0, 0.0),
                (3.0, -1.0, 0.0),
                direction=noon.ORIGIN,
                buff=0.1,
                sharpness=2.5,
            )

            assert calls == [
                ("brace", 0.0, -1.0, 0.3, 1.5),
                ("between", -2.0, 1.0, 3.0, -1.0, 0.0, 0.0, 0.1, 2.5),
            ]
            assert brace.buff == 0.3
            assert brace.sharpness == 1.5
            assert brace.direction == noon.DOWN
            assert brace.style["stroke_width"] == 0.0
            assert brace.style["fill"]["alpha"] == 1.0
            assert brace.style["fill"]["red"] == noon.BLUE.red
            assert between.buff == 0.1
            assert between.sharpness == 2.5
            assert "Brace" in noon.__all__
            assert "BraceBetweenPoints" in noon.__all__

            before = list(calls)
            for thunk in (
                lambda: Brace(target, buff=float("nan")),
                lambda: Brace(target, background_stroke_width=1.0),
                lambda: BraceBetweenPoints((0, 0, 1), (1, 0, 0)),
            ):
                try:
                    thunk()
                except (TypeError, ValueError, NotImplementedError):
                    pass
                else:
                    raise AssertionError("invalid Brace input unexpectedly succeeded")
            assert calls == before
            """
        )

    def test_brace_tip_placement_and_text_composite_are_thin(self) -> None:
        self._run_source(
            r"""
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            import _typed_geometry_test_support as support

            class FakeHandle:
                def __init__(self, snapshot):
                    self.snapshot = snapshot
                def snapshotJson(self):
                    return json.dumps(self.snapshot)
                def beginBrace(self, *args):
                    value = support.FakeGeometryOptions({
                        "vector_path": {"commands": []}
                    })
                    value.setStrokeWidth(0.0)
                    value.setFillOpacity(1.0)
                    return value

            support.install_js_bridge(
                fake_js, lambda value: FakeHandle(json.loads(value))
            )
            sys.modules["js"] = fake_js

            import noon
            import _manim_brace as brace_module
            from noon import Brace, Square

            brace = Brace(Square(2.0))
            anchors = [noon.Vec2(float(index), 0.0) for index in range(8)]
            anchors[7] = noon.Vec2(0.0, -2.0)
            brace.get_anchors = lambda: anchors
            brace.get_center = lambda: noon.ORIGIN

            assert brace.get_tip() == noon.Vec2(0.0, -2.0)
            assert brace.get_direction() == noon.DOWN

            placements = []
            label = object.__new__(noon.Mobject)
            label._scene = None
            label._object = None
            label.next_to = lambda point, direction, **kwargs: placements.append(
                (point, direction, kwargs)
            ) or label
            assert brace.put_at_tip(label, buff=0.4) is brace
            assert placements == [
                (noon.Vec2(0.0, -2.0), noon.DOWN, {"buff": 0.4})
            ]

            for method in (brace.get_text, brace.get_tex):
                try:
                    method("x")
                except NotImplementedError as error:
                    assert "retained text layer" in str(error)
                else:
                    raise AssertionError("Tex/MathTex dependency unexpectedly succeeded")

            class FakeBrace(noon.Mobject):
                def __init__(self, obj, direction=noon.DOWN, buff=0.2, **kwargs):
                    self._scene = None
                    self._object = None
                    self.obj = obj
                    self.direction = noon._as_vec2(direction)
                    self.buff = float(buff)
                    self.kwargs = dict(kwargs)
                    self.placed = []
                def put_at_tip(self, label, **kwargs):
                    self.placed.append((label, dict(kwargs)))
                    return self

            class FakeLabel(noon.Mobject):
                def __init__(self, text, font_size=48.0, **kwargs):
                    self._scene = None
                    self._object = None
                    self.text = text
                    self.font_size = float(font_size)
                    self.kwargs = dict(kwargs)

            family_calls = []
            brace_module.Brace = FakeBrace
            brace_module._compat.VGroup.__init__ = (
                lambda self, *members, z_index=0: family_calls.append(
                    (self, members, float(z_index))
                )
            )
            noon.Text = FakeLabel

            target = object.__new__(noon.Mobject)
            target._scene = None
            target._object = None
            composite = brace_module.BraceText(
                target,
                "Label",
                font_size=36,
                buff=0.3,
                brace_config={"sharpness": 1.5},
            )
            assert composite.label.text == "Label"
            assert composite.label.font_size == 36.0
            assert composite.brace.direction == noon.DOWN
            assert composite.brace.buff == 0.3
            assert composite.brace.kwargs == {"sharpness": 1.5}
            assert composite.brace.placed == [(composite.label, {})]
            assert family_calls[-1][1] == (composite.brace, composite.label)

            explicit = brace_module.BraceLabel(
                target,
                "Plain",
                label_constructor=FakeLabel,
            )
            assert explicit.label.text == "Plain"

            try:
                brace_module.BraceLabel(target, "math")
            except NotImplementedError as error:
                assert "MathTex" in str(error)
            else:
                raise AssertionError("BraceLabel default must wait for MathTex")
            """
        )


if __name__ == "__main__":
    unittest.main()
