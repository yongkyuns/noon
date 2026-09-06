import os
from pathlib import Path
import subprocess
import sys
import textwrap
import unittest


class OrdinaryTrackerScopeTests(unittest.TestCase):
    def test_public_factory_scope_isolation(self):
        # Wrapper installation mutates the public Python classes. Keep this
        # browser-bridge test isolated from native document fixture tests.
        source = textwrap.dedent("""
            import asyncio
            from types import ModuleType
            import unittest
            from unittest.mock import patch

            # The scope tests do not resolve animations; only the browser import needs a stub.
            class Handle:
                def __init__(self, value):
                    self.value = float(value)

                def detachedValue(self):
                    return self.value

                def setDetachedValue(self, value):
                    self.value = float(value)


            bridge = ModuleType("js")
            bridge.noonResolveAnimationOptions = object()
            bridge.noonCreateAuthoringValueTrackerHandle = Handle
            with patch.dict("sys.modules", {"js": bridge}):
                import _manim_reactive as reactive
            ValueTracker = reactive.ValueTracker


            class Context:
                def __init__(self, *, reject=False):
                    self.reject = reject
                    self.associations = []

                def createValueTracker(self, value):
                    handle = Handle(value)
                    handle.owner = self
                    return handle

                def associateValueTracker(self, handle):
                    if self.reject:
                        raise RuntimeError("association rejected")
                    self.associations.append(handle)

                def valueTrackerValue(self, handle):
                    return handle.value


            class Scene:
                def __init__(self, context=None):
                    self.context = context or Context()

                def value_tracker(self, value):
                    return ValueTracker._from_canonical(
                        self, self.context, self.context.createValueTracker(value)
                    )


            class OrdinaryTrackerScopeTests(unittest.IsolatedAsyncioTestCase):
                async def test_detached_tracker_adopts_only_after_shared_commit(self):
                    tracker = ValueTracker(1.25)
                    self.assertIsNone(tracker._scene)
                    self.assertFalse(hasattr(tracker, "_value"))
                    self.assertFalse(hasattr(tracker, "_signal_id"))
                    self.assertEqual(tracker.get_value(), 1.25)
                    tracker.increment_value(0.75)
                    self.assertEqual(tracker.get_value(), 2.0)

                    rejected = Scene(Context(reject=True))
                    with self.assertRaisesRegex(ValueError, "association rejected"):
                        tracker._associate_canonical(rejected, rejected.context)
                    self.assertIsNone(tracker._scene)
                    self.assertIsNone(tracker._canonical_context)

                    scene = Scene()
                    tracker._associate_canonical(scene, scene.context)
                    self.assertIs(tracker._scene, scene)
                    self.assertIs(tracker._canonical_context, scene.context)
                    self.assertFalse(hasattr(tracker, "_value"))
                    self.assertFalse(hasattr(tracker, "_signal_id"))
                    self.assertEqual(scene.context.associations, [tracker._canonical_handle])

                    other = Scene()
                    with self.assertRaisesRegex(ValueError, "another Scene"):
                        tracker._associate_canonical(other, other.context)
                    self.assertEqual(other.context.associations, [])

                async def test_suspended_sources_keep_separate_tracker_owners(self):
                    ready = asyncio.Event()
                    entered = []

                    async def author(scene):
                        token = reactive._enter_authoring_scene(scene)
                        try:
                            entered.append(scene)
                            if len(entered) == 2:
                                ready.set()
                            await ready.wait()
                            return ValueTracker(3)
                        finally:
                            reactive._leave_authoring_scene(token)

                    scenes = [Scene(), Scene()]
                    trackers = await asyncio.gather(*(author(scene) for scene in scenes))
                    for scene, tracker in zip(scenes, trackers):
                        self.assertIs(tracker._scene, scene)
                        self.assertIs(tracker._canonical_context, scene.context)
                        self.assertIs(tracker._canonical_handle.owner, scene.context)
                        self.assertFalse(hasattr(tracker, "_value"))
                        self.assertFalse(hasattr(tracker, "_signal_id"))
                        self.assertEqual(tracker.get_value(), 3.0)
                    self.assertIsNone(reactive._current_authoring_scene())

                async def test_nested_scope_restores_owner_after_exception(self):
                    outer, inner = Scene(), Scene()
                    outer_token = reactive._enter_authoring_scene(outer)
                    try:
                        with self.assertRaisesRegex(RuntimeError, "source failed"):
                            inner_token = reactive._enter_authoring_scene(inner)
                            try:
                                self.assertIs(ValueTracker(1)._scene, inner)
                                raise RuntimeError("source failed")
                            finally:
                                reactive._leave_authoring_scene(inner_token)
                        self.assertIs(ValueTracker(2)._scene, outer)
                    finally:
                        reactive._leave_authoring_scene(outer_token)
                    self.assertIsNone(reactive._current_authoring_scene())


            if __name__ == "__main__":
                unittest.main()
        """)
        python_dir = Path(__file__).resolve().parent
        result = subprocess.run(
            [sys.executable, "-c", source],
            cwd=python_dir,
            env={**os.environ, "PYTHONPATH": str(python_dir)},
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
