import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class ManimSharedObjectObservationTests(unittest.TestCase):
    def test_typed_observations_route_to_authored_and_effective_rust_owners(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing
            else os.pathsep.join((str(python_dir), existing))
        )
        source = textwrap.dedent(
            r"""
            import json
            from test_noon_errors import diagnostic, js_exception
            from types import SimpleNamespace
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = lambda *args: None

            class Handle:
                def __init__(self, snapshot_json):
                    self.snapshot = json.loads(snapshot_json)

                def snapshotJson(self):
                    raise AssertionError("typed observation requested a snapshot")

                def manimLineEndpoints(self):
                    if "line" not in self.snapshot["geometry"]:
                        raise js_exception(diagnostic("unsupported_operation", "authoring.unsupported",
                            "Line endpoint queries require an analytic Line",
                            cause=diagnostic("unsupported_operation", "authoring.unsupported_operation")))
                    return SimpleNamespace(startX=1.25, startY=-2.5, endX=4.5, endY=3.75)

                @property
                def fillOpacity(self): return 0.25
                @property
                def strokeOpacity(self): return 0.75

                def manimColor(self):
                    return SimpleNamespace(red=0.1, green=0.2, blue=0.3, alpha=0.4)

            import _typed_geometry_test_support as geometry_test
            geometry_test.install_js_bridge(fake_js, Handle)
            sys.modules["js"] = fake_js

            import _manim_compat

            import _manim_rate_functions as rate_functions
            import _manim_geometry
            import _manim_semantic_handles as handles


            from noon import BLUE, Line, Square

            line = Line((-1.0, 0.0), (1.0, 0.0))
            line._current_raw = lambda: (_ for _ in ()).throw(
                AssertionError("typed observation used the raw projection")
            )
            assert line.get_start() == (1.25, -2.5)
            assert line.get_end() == (4.5, 3.75)
            assert line.get_fill_opacity() == 0.25
            assert line.get_stroke_opacity() == 0.75
            authored_color = line.get_color()
            assert (
                authored_color.red,
                authored_color.green,
                authored_color.blue,
                authored_color.alpha,
            ) == (0.1, 0.2, 0.3, 0.4)

            class DetachedLiveContext:
                def liveExecutionOwnership(self): return "returned"
                def queryMobjectLineEndpoints(self, handle):
                    raise AssertionError("detached identity was queried through execution")
                def queryMobjectColor(self, handle):
                    raise AssertionError("detached identity was queried through execution")

            line._canonical_live_target_context = DetachedLiveContext()
            assert line.get_start() == (1.25, -2.5)
            assert line.get_end() == (4.5, 3.75)
            assert line.get_color().alpha == 0.4

            import _manim_indication
            flash = _manim_indication.ShowPassingFlash(line)
            assert flash.mobject is line
            try:
                _manim_indication.ShowPassingFlash(Square())
            except NotImplementedError as error:
                assert error.category == "unsupported_operation"
                assert error.code == "authoring.unsupported"
            else:
                raise AssertionError("typed non-Line passed ShowPassingFlash admission")

            class EffectiveContext:
                def __init__(self): self.ownership = "returned"
                def liveExecutionOwnership(self): return self.ownership
                def queryMobjectLineEndpoints(self, handle):
                    if self.ownership == "transferred":
                        raise RuntimeError("live execution session is running")
                    assert handle is line._semantic_handle
                    return SimpleNamespace(startX=10.0, startY=20.0, endX=30.0, endY=40.0)
                def queryMobjectFillOpacity(self, handle):
                    if self.ownership == "transferred":
                        raise RuntimeError("live execution session is running")
                    assert handle is line._semantic_handle
                    return 0.4
                def queryMobjectStrokeOpacity(self, handle):
                    assert handle is line._semantic_handle
                    return 0.6
                def queryMobjectColor(self, handle):
                    assert handle is line._semantic_handle
                    return SimpleNamespace(red=0.8, green=0.7, blue=0.6, alpha=0.5)

            context = EffectiveContext()
            line._scene = SimpleNamespace(
                _canonical_authoring_context=context,
                _tracks=[],
            )
            line._object = SimpleNamespace(id="line")
            assert line.get_start() == (10.0, 20.0)
            assert line.get_end() == (30.0, 40.0)
            line._noon_updaters = [lambda *_: None]
            assert line.get_fill_opacity() == 0.4
            assert line.get_stroke_opacity() == 0.6
            effective_color = line.get_color()
            assert (
                effective_color.red,
                effective_color.green,
                effective_color.blue,
                effective_color.alpha,
            ) == (0.8, 0.7, 0.6, 0.5)

            # Callback reads use the current ordered row and never query the
            # transferred player or authored handle. Reads see prior writes.
            import _manim_updaters as updaters
            phase = object.__new__(updaters._CanonicalCallbackContext)
            row = SimpleNamespace(style=SimpleNamespace(fill=None, stroke=(1, 1, 1, 0.125)))
            phase.row = lambda value: ((0, 0), row)
            updaters._ACTIVE_CONTEXTS[id(line._scene)] = phase
            try:
                assert line.get_fill_opacity() == 0.0
                assert line.get_stroke_opacity() == 0.125
                row.style.fill = (0, 1, 0, 0.375)
                assert line.get_fill_opacity() == 0.375
            finally:
                del updaters._ACTIVE_CONTEXTS[id(line._scene)]

            context.ownership = "transferred"
            try:
                line.get_fill_opacity()
            except RuntimeError as error:
                assert "execution session is running" in str(error)
            else:
                raise AssertionError("paint getter bypassed transferred ownership")
            try:
                _manim_indication.ShowPassingFlash(line)
            except RuntimeError as error:
                assert "execution session is running" in str(error)
            else:
                raise AssertionError("ShowPassingFlash bypassed transferred ownership")
            context.ownership = "returned"

            del line._noon_updaters
            calls = []
            original_set_color = handles._set_color
            handles._set_color = lambda target, color: calls.append((target, color)) or target
            try:
                assert line.set_color(BLUE) is line
            finally:
                handles._set_color = original_set_color
            assert calls == [(line, BLUE)]

            line._semantic_handle_fresh = False
            try:
                line.get_fill_opacity()
            except NotImplementedError as error:
                assert "valid semantic handle" in str(error)
            else:
                raise AssertionError("half-typed paint getter fell back to raw geometry")
            try:
                line.get_start()
            except NotImplementedError as error:
                assert "valid semantic handle" in str(error)
            else:
                raise AssertionError("half-typed Line fell back to a raw snapshot")
            """
        )
        completed = subprocess.run(
            [sys.executable, "-c", source],
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


if __name__ == "__main__":
    unittest.main()
