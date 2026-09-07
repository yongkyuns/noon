import sys
import types
import unittest
from unittest import mock

fake_js = types.ModuleType("js")
fake_js.noonResolveAnimationOptions = object()
fake_js.noonResolveCompositionSchedule = object()
fake_js.noonResolveLifecyclePlan = object()
fake_js.noonResolveUniformCompositionSchedule = object()
fake_js.noonValidatePresenceTransition = object()
sys.modules.setdefault("js", fake_js)

import _manim_compat  # noqa: E402

_manim_compat.install()
import _manim_phase_b  # noqa: E402, F401
import _manim_typst  # noqa: E402

_manim_typst.install()
import _manim_canonical_scene as canonical  # noqa: E402


class ManimLiveExecutionDurationTests(unittest.TestCase):
    def test_default_helper_preserves_existing_handoff_duration(self) -> None:
        class Context:
            def __init__(self) -> None:
                self.durations = []

            def liveHandoffDuration(self):
                return 4.0

            def beginLiveExecution(self, duration):
                self.durations.append(float(duration))

        context = Context()
        scene = object()
        with mock.patch.object(canonical, "execution_context", return_value=context):
            canonical.LiveExecution(scene)
            canonical.LiveExecution(scene, 6.0)

        self.assertEqual(context.durations, [4.0, 6.0])

    def test_first_default_helper_uses_positive_bootstrap_duration(self) -> None:
        class Context:
            def __init__(self) -> None:
                self.duration = None

            def liveHandoffDuration(self):
                return 0.0

            def beginLiveExecution(self, duration):
                self.duration = float(duration)

        context = Context()
        with mock.patch.object(canonical, "execution_context", return_value=context):
            canonical.LiveExecution(object())

        self.assertEqual(context.duration, 1.0)


if __name__ == "__main__":
    unittest.main()
