import unittest
import inspect
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class StyleOperationsTests(unittest.TestCase):
    def test_set_style_exposes_the_pinned_options_without_a_catch_all_kwargs(self):
        parameters = list(inspect.signature(compat.VMobject.set_style).parameters)
        self.assertEqual(parameters, [
            "self",
            "fill_color",
            "fill_opacity",
            "stroke_color",
            "stroke_width",
            "stroke_opacity",
            "background_stroke_color",
            "background_stroke_width",
            "background_stroke_opacity",
            "sheen_factor",
            "sheen_direction",
            "background_image",
            "family",
        ])

    def test_combined_style_is_one_typed_request_and_validates_before_dispatch(self):
        obj = identity_only_wrapper(compat.Circle)
        bridge = SimpleNamespace(setStyle=Mock())
        with patch.object(handles, '_style_target', return_value=(bridge, None, '')):
            self.assertIs(obj.set_style(fill_color='#FF0000', fill_opacity=0.4,
                                        stroke_color='#0000FF', stroke_width=6), obj)
            bridge.setStyle.assert_called_once_with(True, 1, 0, 0, 1, 0.4,
                                                   True, 0, 0, 1, 1, 0.06, None)
            with self.assertRaises(ValueError):
                obj.set_style(fill_color='#00FF00', stroke_opacity=2)
            self.assertEqual(bridge.setStyle.call_count, 1)
            with self.assertRaises(NotImplementedError):
                obj.set_style(fill_color='#00FF00', background_stroke_opacity=0.5)
            self.assertEqual(bridge.setStyle.call_count, 1)
        with self.assertRaises(TypeError):
            obj.set_style(unrecognized_option=True)

    def test_matching_uses_live_family_route_and_rejects_unsupported_options(self):
        source = identity_only_wrapper(compat.VGroup)
        target = identity_only_wrapper(compat.VGroup)
        source._semantic_family_handle = object()
        target._semantic_family_handle = object()
        source_handle, target_handle = object(), object()
        context = SimpleNamespace(liveMatchFamilyStyle=Mock())
        with patch.object(handles, '_style_target', side_effect=[
                (source_handle, context, 'Family'), (target_handle, context, 'Family')]):
            self.assertIs(source.match_style(target), source)
        context.liveMatchFamilyStyle.assert_called_once_with(source_handle, target_handle)
        with self.assertRaises(NotImplementedError):
            source.match_style(target, family=False)
        with self.assertRaises(NotImplementedError):
            source.set_style(sheen_factor=0.5)

    def test_omitted_leaf_paint_does_not_dispatch_disable(self):
        obj = identity_only_wrapper(compat.Circle)
        bridge, context = Mock(), Mock()
        for live in [None, context]:
            with patch.object(handles, '_handle_for', return_value=bridge), \
                 patch.object(handles, '_live_mutation_context', return_value=live):
                self.assertIs(handles._set_fill(obj), obj)
                self.assertIs(handles._set_stroke(obj), obj)
        bridge.disableFill.assert_not_called()
        bridge.disableStroke.assert_not_called()
        context.liveDisableFill.assert_not_called()
        context.liveDisableStroke.assert_not_called()
