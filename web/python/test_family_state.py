import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class FamilyStateTests(unittest.TestCase):
    def test_become_and_restore_forward_family_handles_and_global_options(self):
        source = identity_only_wrapper(compat.VGroup)
        target = identity_only_wrapper(compat.VGroup)
        source._semantic_family_handle = SimpleNamespace(becomeFamily=Mock())
        target._semantic_family_handle = object()
        source.saved_state = target
        with patch.object(handles, '_group_target_context', return_value=None):
            self.assertIs(source.become(target, match_width=True, match_center=True), source)
            source._semantic_family_handle.becomeFamily.assert_called_once_with(
                target._semantic_family_handle, False, True, True, False)
            source._semantic_family_handle.becomeFamily.reset_mock()
            self.assertIs(source.restore(), source)
            source._semantic_family_handle.becomeFamily.assert_called_once_with(
                target._semantic_family_handle, False, False, False, False)

    def test_live_family_state_uses_one_publication_call(self):
        source = identity_only_wrapper(compat.Group)
        target = identity_only_wrapper(compat.Group)
        source._semantic_family_handle = object()
        target._semantic_family_handle = object()
        context = SimpleNamespace(liveBecomeFamily=Mock())
        with patch.object(handles, '_group_target_context', return_value=context):
            source.become(target, stretch=True)
        context.liveBecomeFamily.assert_called_once_with(
            source._semantic_family_handle, target._semantic_family_handle,
            False, False, False, True)
        with self.assertRaises(NotImplementedError):
            source.become(identity_only_wrapper(compat.Mobject))
