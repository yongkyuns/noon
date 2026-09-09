import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class MovingDotsPrimitiveTests(unittest.TestCase):
    def test_tracker_callback_reads_and_raw_line_matching_rejection(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )

        source = textwrap.dedent(
            """
            import json
            import sys
            import types

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = lambda *args: None
            sys.modules["js"] = fake_js

            from _typed_geometry_test_support import identity_only_wrapper as identity
            import _manim_compat as manim
            import _manim_geometry  # noqa: F401 - installs match_points/layout semantics
            import _manim_reactive as reactive
            import _manim_updaters as updaters
            import noon as api

            updaters.install()

            # Read the pinned callback view during the ordered phase and the
            # shared published value afterwards; Python owns neither value.
            from unittest.mock import patch
            scene, handle = object(), object()
            context = types.SimpleNamespace(valueTrackerValue=lambda value: 0.0)
            tracker = reactive.ValueTracker._from_canonical(scene, context, handle)
            assert not hasattr(tracker, "_value")
            assert not hasattr(tracker, "_signal_id")
            def read_phase(owner, value):
                assert owner is scene and value is handle
                return 2.25
            token = updaters._ACTIVE_CANONICAL_CONTEXT.set(object())
            try:
                with patch.object(updaters, "canonical_callback_scalar_value", read_phase):
                    assert tracker.get_value() == 2.25
                try:
                    tracker.set_value(4)
                except NotImplementedError:
                    pass
                else:
                    raise AssertionError("callback writes must remain explicit")
            finally:
                updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
            assert tracker.get_value() == 0.0

            # Raw geometry replacement is no longer a callback compatibility path.
            # The canonical opaque-handle proof lives in test_canonical_line_match.
            source_line = identity(manim.Line)
            target_line = identity(manim.Line)
            try:
                source_line.match_points(target_line)
            except NotImplementedError as error:
                assert "opaque shared semantic Line handles" in str(error)
            else:
                raise AssertionError("raw Line geometry matching must not remain available")
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
            f"MovingDots primitive subprocess failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}",
        )


if __name__ == "__main__":
    unittest.main()
