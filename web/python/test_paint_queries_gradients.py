import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class PaintQueriesGradientsTests(unittest.TestCase):
    def test_channel_queries_use_typed_observations_and_exclude_opacity(self):
        obj = identity_only_wrapper(compat.Circle)
        color = SimpleNamespace(red=1.0, green=0.0, blue=0.0, alpha=0.2)
        with patch.object(handles, '_typed_manim_observation', side_effect=[color, color, 0.06]) as query:
            self.assertEqual(obj.get_fill_color().alpha, 1.0)
            self.assertEqual(obj.get_stroke_color().red, 1.0)
            self.assertEqual(obj.get_stroke_width(), 6.0)
        self.assertEqual([call.args[1] for call in query.call_args_list],
                         ['fillColor', 'strokeColor', 'strokeWidth'])

    def test_gradient_is_one_shared_family_request_without_python_interpolation(self):
        group = identity_only_wrapper(compat.VGroup)
        handle = object()
        context = SimpleNamespace(liveSetFamilyColorGradient=Mock())
        argument = object()
        with patch.object(handles, '_style_target', return_value=(handle, context, 'Family')), \
             patch.object(handles, '_gradient_components', return_value=argument) as components:
            self.assertIs(group.set_color_by_gradient('#FF0000', '#0000FF'), group)
        context.liveSetFamilyColorGradient.assert_called_once_with(handle, argument)
        self.assertEqual(len(components.call_args.args[0]), 2)
        self.assertEqual(components.call_args.args[0][0].red, 1.0)
        self.assertEqual(components.call_args.args[0][1].blue, 1.0)
