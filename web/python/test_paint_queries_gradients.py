import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
import noon
from _typed_geometry_test_support import identity_only_wrapper


class PaintQueriesGradientsTests(unittest.TestCase):
    def test_match_color_preserves_observation_before_dynamic_setter_and_returns_result(self):
        calls = []
        color = object()
        class Donor:
            def get_color(self):
                calls.append("get")
                return color
        class Recipient(noon.Mobject):
            def __init__(self): pass
            def set_color(self, value):
                calls.append(("set", value))
                return self
        recipient = Recipient()
        self.assertIs(recipient.match_color(Donor()), recipient)
        self.assertEqual(calls, ["get", ("set", color)])
        self.assertIs(compat.Group.match_color, noon.Mobject.match_color)

    def test_match_color_donor_failure_does_not_call_the_recipient_setter(self):
        recipient = identity_only_wrapper(compat.Circle)
        donor = SimpleNamespace(get_color=Mock(side_effect=RuntimeError("donor unavailable")))
        with patch.object(compat.Circle, "set_color") as setter:
            with self.assertRaisesRegex(RuntimeError, "donor unavailable"):
                recipient.match_color(donor)
        setter.assert_not_called()

    def test_channel_queries_use_typed_observations_and_exclude_opacity(self):
        obj = identity_only_wrapper(compat.Circle)
        color = SimpleNamespace(red=1.0, green=0.0, blue=0.0, alpha=0.2)
        with patch.object(handles, '_paint_color', side_effect=[color, color]), \
             patch.object(compat.Circle, 'get_fill_opacity', return_value=0.3), \
             patch.object(compat.Circle, 'get_stroke_opacity', return_value=0.4), \
             patch.object(handles, '_typed_manim_observation', return_value=0.06) as query:
            self.assertEqual(obj.get_fill_color().alpha, 1.0)
            self.assertEqual(obj.get_stroke_color().red, 1.0)
            self.assertEqual(obj.get_stroke_width(), 6.0)
        self.assertEqual([call.args[1] for call in query.call_args_list],
                         ['strokeWidth'])

    def test_style_array_queries_reconstruct_shared_solid_paint(self):
        obj = identity_only_wrapper(compat.Circle)
        red = noon.Color(1.0, 0.0, 0.0)
        blue = noon.Color(0.0, 0.0, 1.0)
        with patch.object(handles, '_paint_color', side_effect=lambda _obj, channel: red if channel == 'fill' else blue), \
             patch.object(compat.Circle, 'get_fill_opacity', return_value=0.25), \
             patch.object(compat.Circle, 'get_stroke_opacity', return_value=0.75):
            fill_rgbas = obj.get_fill_rgbas()
            stroke_rgbas = obj.get_stroke_rgbas()
            fill_color = obj.get_fill_colors()[0]
            stroke_color = obj.get_stroke_colors()[0]
            self.assertEqual((fill_color.red, fill_color.green, fill_color.blue), (1.0, 0.0, 0.0))
            self.assertEqual((stroke_color.red, stroke_color.green, stroke_color.blue), (0.0, 0.0, 1.0))
            fill_opacities = obj.get_fill_opacities()
            stroke_opacities = obj.get_stroke_opacities()

        self.assertEqual(fill_rgbas.shape, (1, 4))
        self.assertEqual(stroke_rgbas.shape, (1, 4))
        self.assertEqual(str(fill_rgbas.dtype), 'float64')
        self.assertEqual(str(stroke_rgbas.dtype), 'float64')
        self.assertEqual(fill_rgbas.tolist(), [[1.0, 0.0, 0.0, 0.25]])
        self.assertEqual(stroke_rgbas.tolist(), [[0.0, 0.0, 1.0, 0.75]])
        self.assertEqual(fill_opacities.tolist(), [0.25])
        self.assertEqual(stroke_opacities.tolist(), [0.75])

    def test_transparent_black_has_no_manim_fill_color_and_background_strokes_reject(self):
        obj = identity_only_wrapper(compat.Circle)
        black = noon.Color(0.0, 0.0, 0.0)
        with patch.object(handles, '_paint_color', return_value=black), \
             patch.object(compat.Circle, 'get_fill_opacity', return_value=0.0):
            self.assertEqual(obj.get_fill_rgbas().tolist(), [[0.0, 0.0, 0.0, 0.0]])
            self.assertEqual(obj.get_fill_colors(), [None])
            self.assertIsNone(obj.get_fill_color())
        with self.assertRaisesRegex(NotImplementedError, 'background strokes require shared background paint'):
            obj.get_stroke_rgbas(background=True)

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
