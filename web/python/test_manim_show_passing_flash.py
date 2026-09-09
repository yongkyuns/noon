import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class ManimShowPassingFlashTests(unittest.TestCase):
    def test_exact_line_request_is_inert_and_rejects_unsupported_options(self) -> None:
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
                    raise AssertionError("PassingFlash requested a snapshot")

                def manimLineEndpoints(self):
                    if "line" not in self.snapshot["geometry"]:
                        raise js_exception(diagnostic("unsupported_operation", "authoring.unsupported",
                            "Line endpoint queries require an analytic Line",
                            cause=diagnostic("unsupported_operation", "authoring.unsupported_operation")))
                    line = self.snapshot["geometry"]["line"]
                    return SimpleNamespace(
                        startX=line["start"]["x"], startY=line["start"]["y"],
                        endX=line["end"]["x"], endY=line["end"]["y"],
                    )

            import _typed_geometry_test_support as geometry_test
            geometry_test.install_js_bridge(fake_js, Handle)
            sys.modules["js"] = fake_js

            import _manim_compat

            import _manim_rate_functions
            import _manim_animate  # noqa: F401
            import _manim_composition
            play_before_composition = _manim_compat.Scene.play
            assert _manim_compat.Scene.play is play_before_composition
            import _manim_semantic_handles as handles

            import _manim_indication

            play_before = _manim_compat.Scene.play
            assert _manim_compat.Scene.play is play_before

            from noon import Line, ShowPassingFlash, Square, linear

            line = Line((-2.0, 0.0), (2.0, 0.0))
            flash = ShowPassingFlash(
                line, time_width=0.25, run_time=3.0, rate_func=linear
            )
            assert flash.mobject is line
            assert flash.target is line
            assert flash.time_width == 0.25
            assert flash.remover is True and flash.introducer is True
            assert flash.anim_args == {"run_time": 3.0, "rate_func": linear}

            try:
                ShowPassingFlash(Square())
            except NotImplementedError:
                pass
            else:
                raise AssertionError("non-Line PassingFlash was accepted")

            for kwargs in (
                {"time_width": 0.0},
                {"lag_ratio": 0.25},
                {"reverse_rate_function": True},
                {"introducer": False},
                {"remover": False},
            ):
                try:
                    ShowPassingFlash(line, **kwargs)
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError(f"unsupported PassingFlash options survived: {kwargs}")
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
