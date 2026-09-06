import asyncio
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import patch

# The scope tests do not resolve animations; only the browser import needs a stub.
bridge = ModuleType("js")
bridge.noonResolveAnimationOptions = object()
with patch.dict("sys.modules", {"js": bridge}):
    import _manim_reactive as reactive
ValueTracker = reactive.ValueTracker


class Context:
    def createValueTracker(self, value):
        return SimpleNamespace(owner=self, value=float(value))

    def valueTrackerValue(self, handle):
        return handle.value


class Scene:
    def __init__(self):
        self.context = Context()

    def value_tracker(self, value):
        return ValueTracker._from_canonical(
            self, self.context, self.context.createValueTracker(value)
        )


class OrdinaryTrackerScopeTests(unittest.IsolatedAsyncioTestCase):
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
