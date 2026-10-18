import unittest
import inspect
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class StyleOperationsTests(unittest.TestCase):
    def test_vgroup_fill_and_opacity_expose_family_and_root_only_dispatch(self):
        self.assertEqual(list(inspect.signature(compat.VGroup.set_fill).parameters), [
            "self", "color", "opacity", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VGroup.set_opacity).parameters), [
            "self", "opacity", "family",
        ])
        self.assertIs(inspect.signature(compat.VGroup.set_fill).parameters["family"].default, True)
        self.assertIs(inspect.signature(compat.VGroup.set_opacity).parameters["family"].default, True)

        group = identity_only_wrapper(compat.VGroup, submobjects=[])
        group._semantic_family_handle = Mock()
        with patch.object(handles, "_group_set_fill", return_value=group) as set_fill:
            self.assertIs(group.set_fill(compat._base.RED, 0.5, family=False), group)
        set_fill.assert_called_once_with(group, compat._base.RED, 0.5, False)
        with patch.object(handles, "_group_set_opacity", return_value=group) as set_opacity:
            self.assertIs(group.set_opacity(0.5, family=False), group)
        set_opacity.assert_called_once_with(group, 0.5, False)

    def test_default_vector_style_mutators_match_the_pinned_surface(self):
        self.assertEqual(list(inspect.signature(compat.VMobject.set_stroke).parameters), [
            "self", "color", "width", "opacity", "background", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VGroup.set_stroke).parameters), [
            "self", "color", "width", "opacity", "background", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.set_background_stroke).parameters), [
            "self", "kwargs",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.set_sheen).parameters), [
            "self", "factor", "direction", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.set_sheen_direction).parameters), [
            "self", "direction", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.rotate_sheen_direction).parameters), [
            "self", "angle", "axis", "family",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.set_cap_style).parameters), [
            "self", "cap_style",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.color_using_background_image).parameters), [
            "self", "background_image",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.match_background_image).parameters), [
            "self", "vmobject",
        ])
        self.assertEqual(list(inspect.signature(compat.VMobject.get_gradient_start_and_end_points).parameters), [
            "self",
        ])
        self.assertIs(compat.VGroup.rotate_sheen_direction, compat.VMobject.rotate_sheen_direction)
        self.assertIs(compat.VGroup.set_cap_style, compat.VMobject.set_cap_style)
        self.assertIs(compat.VGroup.color_using_background_image,
                      compat.VMobject.color_using_background_image)
        self.assertIs(compat.VGroup.match_background_image, compat.VMobject.match_background_image)
        self.assertIs(compat.VGroup.get_background_image, compat.VMobject.get_background_image)
        self.assertIs(compat.VGroup.get_gradient_start_and_end_points,
                      compat.VMobject.get_gradient_start_and_end_points)

        obj = identity_only_wrapper(compat.Circle)
        self.assertIs(obj.set_stroke(background=True), obj)
        self.assertIs(obj.set_background_stroke(), obj)
        self.assertIs(obj.set_cap_style(compat._base.CapStyleType.AUTO), obj)
        self.assertIs(obj.set_sheen(0.0, compat._base.UL), obj)
        self.assertIs(obj.set_sheen_direction(compat._base.UL), obj)
        self.assertIs(obj.rotate_sheen_direction(0.0), obj)
        group = identity_only_wrapper(compat.VGroup)
        self.assertIs(group.rotate_sheen_direction(0.0), group)
        self.assertIs(group.set_cap_style(compat._base.CapStyleType.AUTO), group)
        self.assertIsNone(group.get_background_image())

    def test_none_background_image_helpers_restore_white_through_shared_color(self):
        obj = identity_only_wrapper(compat.VMobject)
        donor = identity_only_wrapper(compat.VMobject)
        with patch.object(compat.VMobject, "set_color", return_value=obj) as set_color:
            self.assertIs(obj.color_using_background_image(None), obj)
            self.assertIs(obj.match_background_image(donor), obj)
        self.assertEqual(set_color.call_count, 2)
        set_color.assert_called_with(compat._base.WHITE)

        with patch.object(compat.VMobject, "set_color") as set_color:
            with self.assertRaises(NotImplementedError):
                obj.color_using_background_image("background.png")
            with patch.object(compat.VMobject, "get_background_image", return_value="background.png"):
                with self.assertRaises(NotImplementedError):
                    obj.match_background_image(donor)
        set_color.assert_not_called()

    def test_gradient_endpoint_query_combines_rust_backed_center_and_edge_queries(self):
        obj = identity_only_wrapper(compat.VMobject)
        center = compat._base.Vec2(1.0, 2.0)
        right = compat._base.Vec2(4.0, 2.0)
        up = compat._base.Vec2(1.0, 5.0)
        with patch.object(compat._base.Mobject, "get_center", return_value=center), \
             patch.object(compat._base.Mobject, "get_edge_center",
                          side_effect=[right, up]) as edge_center:
            start, end = obj.get_gradient_start_and_end_points()

        as_list = lambda value: value.tolist() if hasattr(value, "tolist") else value
        self.assertEqual(as_list(start), [4.0, -1.0, 0.0])
        self.assertEqual(as_list(end), [-2.0, 5.0, 0.0])
        self.assertEqual(
            [call.args[0] for call in edge_center.call_args_list],
            [compat._base.RIGHT, compat._base.UP],
        )

        unsupported = (
            lambda: obj.set_stroke(color=compat._base.RED, width=2, background=True),
            lambda: obj.set_background_stroke(color=compat._base.RED, width=2),
            lambda: obj.set_sheen(0.5),
            lambda: obj.set_sheen_direction(compat._base.UP),
            lambda: obj.rotate_sheen_direction(0.5),
            lambda: obj.set_cap_style(compat._base.CapStyleType.ROUND),
        )
        for operation in unsupported:
            with self.subTest(operation=operation), self.assertRaises(NotImplementedError):
                operation()
        with self.assertRaises(TypeError):
            obj.set_background_stroke(unrecognized_option=True)

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
            with self.assertRaises(NotImplementedError):
                obj.set_style(
                    fill_color='#00FF00',
                    sheen_factor=0.5,
                    sheen_direction=compat._base.UP,
                )
            self.assertEqual(bridge.setStyle.call_count, 1)
            self.assertIs(obj.set_style(sheen_direction=compat._base.UP), obj)
            self.assertIs(obj.set_style(
                sheen_factor=0.0,
                sheen_direction=compat._base.RIGHT,
            ), obj)
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
