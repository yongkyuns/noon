"""Python forwards material authoring to shared Rust operations."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper


class EmissionAuthoringTests(unittest.TestCase):
    def test_leaf_and_family_use_one_shared_operation(self):
        for family in (False, True):
            for live in (False, True):
                obj = identity_only_wrapper(compat.VGroup if family else compat.VMobject)
                handle = Mock()
                if family:
                    obj._semantic_family_handle = handle
                context = Mock() if live else None
                with patch.object(handles, "_handle_for", return_value=handle), \
                     patch.object(handles, "_live_mutation_context", return_value=context), \
                     patch.object(handles, "_live_constructor_context", return_value=None):
                    strength = 7.000000000000001
                    self.assertIs(obj.set_emission(strength), obj)
                if live:
                    method = context.liveSetFamilyEmission if family else context.liveSetEmission
                    method.assert_called_once_with(handle, strength)
                    handle.setEmission.assert_not_called()
                else:
                    handle.setEmission.assert_called_once_with(strength)
                self.assertNotIn("emission", obj.__dict__)

    def test_callback_and_nonfinite_arguments_do_not_author_material_state(self):
        obj = identity_only_wrapper(compat.VMobject)
        with patch.object(handles, "_handle_for") as get_handle:
            for value in (float("nan"), float("inf")):
                with self.assertRaises((ValueError, TypeError)):
                    obj.set_emission(value)
            with patch.object(updaters, "canonical_callback_phase_active", return_value=True):
                with self.assertRaisesRegex(NotImplementedError, "callback emission"):
                    obj.set_emission(7)
            get_handle.assert_not_called()


if __name__ == "__main__":
    unittest.main()
