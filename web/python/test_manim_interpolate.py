import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper


class ManimInterpolateTests(unittest.TestCase):
    def _objects(self):
        objects = [identity_only_wrapper(compat.VMobject) for _ in range(3)]
        for index, value in enumerate(objects):
            value._semantic_handle = Mock(name=f"handle_{index}")
            value._semantic_handle_fresh = True
        return objects

    def test_default_path_uses_one_shared_rust_mutation(self):
        receiver, start, end = self._objects()
        context_handles = {
            receiver: receiver._semantic_handle,
            start: start._semantic_handle,
            end: end._semantic_handle,
        }
        with patch.object(
            handles, "_handle_for", side_effect=lambda value: context_handles[value]
        ), patch.object(handles, "_live_mutation_context", return_value=None), patch.object(
            updaters, "_canonical_phase_context", return_value=None
        ):
            self.assertIs(receiver.interpolate(start, end, 0.25), receiver)

        receiver._semantic_handle.interpolateManim.assert_called_once_with(
            start._semantic_handle, end._semantic_handle, 0.25
        )

    def test_live_interpolation_uses_one_canonical_publication(self):
        receiver, start, end = self._objects()
        context = Mock()
        context_handles = {
            receiver: receiver._semantic_handle,
            start: start._semantic_handle,
            end: end._semantic_handle,
        }
        with patch.object(
            handles, "_handle_for", side_effect=lambda value: context_handles[value]
        ), patch.object(handles, "_live_mutation_context", return_value=context), patch.object(
            updaters, "_canonical_phase_context", return_value=None
        ):
            self.assertIs(receiver.interpolate(start, end, 0.75), receiver)

        context.liveInterpolateMobject.assert_called_once_with(
            receiver._semantic_handle,
            start._semantic_handle,
            end._semantic_handle,
            0.75,
        )
        receiver._semantic_handle.interpolateManim.assert_not_called()

    def test_custom_path_and_callback_context_reject_before_publication(self):
        receiver, start, end = self._objects()
        with self.assertRaisesRegex(NotImplementedError, "custom path_func"):
            receiver.interpolate(start, end, 0.5, path_func=lambda a, b, t: a)
        receiver._semantic_handle.interpolateManim.assert_not_called()

        with patch.object(updaters, "_canonical_phase_context", return_value=object()):
            with self.assertRaisesRegex(NotImplementedError, "host callbacks"):
                receiver.interpolate(start, end, 0.5)
        receiver._semantic_handle.interpolateManim.assert_not_called()

    def test_custom_interpolate_color_override_rejects_before_geometry_mutation(self):
        class CustomVMobject(compat.VMobject):
            def interpolate_color(self, mobject1, mobject2, alpha):
                return self

        receiver = identity_only_wrapper(CustomVMobject)
        receiver._semantic_handle = Mock()
        receiver._semantic_handle_fresh = True
        start, end = self._objects()[1:]
        with self.assertRaisesRegex(NotImplementedError, "subclass interpolate_color"):
            receiver.interpolate(start, end, 0.5)
        receiver._semantic_handle.interpolateManim.assert_not_called()

    def test_base_mobject_keeps_pinned_override_error(self):
        object_ = identity_only_wrapper(compat.Mobject)
        with self.assertRaisesRegex(
            NotImplementedError, r"Please override in a child class\."
        ):
            object_.interpolate(object_, object_, 0.5)


if __name__ == "__main__":
    unittest.main()
