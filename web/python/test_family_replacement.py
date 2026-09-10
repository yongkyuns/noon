"""Replacement sends one shared layout operation for objects and families."""
import types
import unittest
from unittest.mock import patch

import noon
import _manim_semantic_handles as semantic


class FamilyReplacementTests(unittest.TestCase):
    def setUp(self):
        self.calls = []
        self.source = object.__new__(noon.Group)
        self.target = object.__new__(noon.VGroup)
        self.source_anchor = types.SimpleNamespace(
            replaceLayout=lambda *args: self.calls.append(args))
        self.target_anchor = object()

    def test_detached_families_use_one_shared_transaction(self):
        with patch.object(semantic, "_dimension_fit_source", return_value=(self.source_anchor, None)), \
             patch.object(semantic, "_layout_anchor", return_value=self.target_anchor), \
             patch.object(semantic, "_group_live_layout_context", return_value=None), \
             patch.object(semantic, "_live_constructor_context", return_value=None):
            self.assertIs(self.source.replace(self.target, dim_to_match=1, stretch=True), self.source)
        self.assertEqual(self.calls, [(self.target_anchor, 1, True)])

    def test_live_or_detached_source_uses_the_available_publication_context(self):
        context = types.SimpleNamespace(liveReplaceLayout=lambda *args: self.calls.append(args))
        for source_context in (context, None):
            with patch.object(semantic, "_dimension_fit_source", return_value=(self.source_anchor, source_context)), \
                 patch.object(semantic, "_layout_anchor", return_value=self.target_anchor), \
                 patch.object(semantic, "_group_live_layout_context", return_value=context):
                self.assertIs(self.source.replace(self.target), self.source)
        self.assertEqual(self.calls, [(self.source_anchor, self.target_anchor, 0, False)] * 2)

    def test_invalid_target_is_rejected_before_any_mutation(self):
        with patch.object(semantic, "_dimension_fit_source") as prepare:
            with self.assertRaises(TypeError):
                self.source.replace(object())
            prepare.assert_not_called()
        self.assertEqual(self.calls, [])
