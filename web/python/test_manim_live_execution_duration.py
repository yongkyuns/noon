import os
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path


class ManimLiveExecutionDurationTests(unittest.TestCase):
    def test_default_helper_preserves_shared_handoff_duration(self) -> None:
        python_dir = Path(__file__).resolve().parent
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        existing_pythonpath = env.get("PYTHONPATH")
        env["PYTHONPATH"] = (
            str(python_dir)
            if not existing_pythonpath
            else os.pathsep.join((str(python_dir), existing_pythonpath))
        )
        source = textwrap.dedent(
            """
            import sys
            import types
            from unittest import mock

            fake_js = types.ModuleType("js")
            fake_js.noonResolveAnimationOptions = object()
            fake_js.noonResolveCompositionSchedule = object()
            fake_js.noonResolveLifecyclePlan = object()
            fake_js.noonResolveUniformCompositionSchedule = object()
            fake_js.noonValidatePresenceTransition = object()
            sys.modules["js"] = fake_js

            import _manim_compat
            _manim_compat.install()
            import _manim_phase_b  # noqa: F401
            import _manim_typst
            _manim_typst.install()
            import _manim_canonical_scene as canonical

            class Context:
                def __init__(self, handoff):
                    self.handoff = handoff
                    self.durations = []

                def liveHandoffDuration(self):
                    return self.handoff

                def beginLiveExecution(self, duration):
                    self.durations.append(float(duration))

            returned = Context(4.0)
            with mock.patch.object(canonical, "execution_context", return_value=returned):
                canonical.LiveExecution(object())
                canonical.LiveExecution(object(), 6.0)
            assert returned.durations == [4.0, 6.0]

            presegment = Context(0.0)
            with mock.patch.object(canonical, "execution_context", return_value=presegment):
                canonical.LiveExecution(object())
            assert presegment.durations == [1.0]
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
