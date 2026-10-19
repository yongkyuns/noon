import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper


class ManimInterpolateColorTests(unittest.TestCase):
    def _objects(self):
        objects = [identity_only_wrapper(compat.VMobject) for _ in range(3)]
        for index, value in enumerate(objects):
            value._semantic_handle = Mock(name=f"handle_{index}")
            value._semantic_handle_fresh = True
        return objects

    def test_detached_interpolation_uses_one_shared_rust_mutation(self):
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
            self.assertIs(receiver.interpolate_color(start, end, 0.25), receiver)

        receiver._semantic_handle.interpolateManimStyle.assert_called_once_with(
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
            self.assertIs(receiver.interpolate_color(start, end, 0.75), receiver)

        context.liveInterpolateMobjectStyle.assert_called_once_with(
            receiver._semantic_handle,
            start._semantic_handle,
            end._semantic_handle,
            0.75,
        )
        receiver._semantic_handle.interpolateManimStyle.assert_not_called()

    def test_callback_and_family_targets_are_explicitly_unsupported(self):
        receiver, start, end = self._objects()
        with patch.object(updaters, "_canonical_phase_context", return_value=object()):
            with self.assertRaisesRegex(NotImplementedError, "host callbacks"):
                receiver.interpolate_color(start, end, 0.5)

        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        with patch.object(updaters, "_canonical_phase_context", return_value=None):
            with self.assertRaisesRegex(NotImplementedError, "individual VMobjects"):
                receiver.interpolate_color(family, end, 0.5)

    def test_base_mobject_keeps_pinned_override_error(self):
        object_ = identity_only_wrapper(compat.Mobject)
        with self.assertRaisesRegex(
            NotImplementedError, r"Please override in a child class\."
        ):
            object_.interpolate_color(object_, object_, 0.5)


if __name__ == "__main__":
    unittest.main()
