import builtins
import inspect
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import _manim_compat as compat
import _manim_semantic_handles as handles
from _typed_geometry_test_support import identity_only_wrapper


class StyleOperationsTests(unittest.TestCase):
    def test_planar_shading_profile_is_shared_without_python_state(self):
        self.assertEqual(
            list(inspect.signature(compat.VMobject.set_shade_in_3d).parameters),
            ["self", "value", "z_index_as_group"],
        )
        self.assertEqual(
            list(inspect.signature(compat.VGroup.__init__).parameters),
            ["self", "mobjects", "z_index", "color", "shade_in_3d"],
        )
        self.assertIs(compat.VGroup.shade_in_3d, compat.VMobject.shade_in_3d)
        self.assertIs(compat.VGroup.set_shade_in_3d, compat.VMobject.set_shade_in_3d)

        with patch.object(handles, "_empty_path_init") as initialize_path:
            vector = compat.VMobject(shade_in_3d=False)
        initialize_path.assert_called_once()
        self.assertIs(vector.shade_in_3d, False)
        self.assertIs(vector.set_shade_in_3d(False), vector)
        vector.shade_in_3d = False
        self.assertIs(vector.shade_in_3d, False)

        with patch.object(handles, "_group_init") as initialize_group:
            family = compat.VGroup(shade_in_3d=False)
        initialize_group.assert_called_once()
        self.assertIs(family.shade_in_3d, False)
        self.assertIs(family.set_shade_in_3d(False), family)

    def test_unsupported_3d_shading_rejects_before_initialization(self):
        with patch.object(handles, "_empty_path_init") as initialize_path:
            with self.assertRaisesRegex(NotImplementedError, "3D-shaded"):
                compat.VMobject(shade_in_3d=True)
        initialize_path.assert_not_called()

        vector = identity_only_wrapper(compat.VMobject)
        family = identity_only_wrapper(compat.VGroup)
        for operation in (
            lambda: setattr(vector, "shade_in_3d", True),
            lambda: vector.set_shade_in_3d(True),
            lambda: vector.set_shade_in_3d(False, z_index_as_group=True),
            lambda: family.set_shade_in_3d(True),
        ):
            with self.assertRaises(NotImplementedError):
                operation()

        with patch.object(handles, "_group_init") as initialize_group:
            with self.assertRaisesRegex(NotImplementedError, "3D-shaded"):
                compat.VGroup(shade_in_3d=True)
        initialize_group.assert_not_called()

    def test_generate_rgbas_array_pairs_colors_and_opacities(self):
        self.assertEqual(
            list(inspect.signature(compat.VMobject.generate_rgbas_array).parameters),
            ["self", "color", "opacity"],
        )
        self.assertIs(
            compat.VGroup.generate_rgbas_array,
            compat.VMobject.generate_rgbas_array,
        )

        vector = identity_only_wrapper(compat.VMobject)
        family = identity_only_wrapper(compat.VGroup)
        family_rgbas = family.generate_rgbas_array(compat._base.BLUE, 0.3)
        family_values = (
            family_rgbas.tolist()
            if hasattr(family_rgbas, "tolist")
            else family_rgbas
        )
        self.assertEqual(
            family_values,
            [
                [
                    compat._base.BLUE.red,
                    compat._base.BLUE.green,
                    compat._base.BLUE.blue,
                    0.3,
                ]
            ],
        )

        generated = vector.generate_rgbas_array(
            [None, "#FFFF00"], [None, 0.6]
        )
        values = generated.tolist() if hasattr(generated, "tolist") else generated
        self.assertEqual(values, [[0.0, 0.0, 0.0, 0.0], [1.0, 1.0, 0.0, 0.6]])
        if hasattr(generated, "dtype"):
            self.assertEqual(str(generated.dtype), "float64")
            self.assertEqual(list(generated.shape), [2, 4])

        repeated_color = vector.generate_rgbas_array("RED", [0.2, 0.8])
        repeated_values = (
            repeated_color.tolist()
            if hasattr(repeated_color, "tolist")
            else repeated_color
        )
        self.assertEqual(
            repeated_values,
            [
                [
                    compat._base.RED.red,
                    compat._base.RED.green,
                    compat._base.RED.blue,
                    0.2,
                ],
                [
                    compat._base.RED.red,
                    compat._base.RED.green,
                    compat._base.RED.blue,
                    0.8,
                ],
            ],
        )

        numeric_colors = vector.generate_rgbas_array(
            [(0.1, 0.25, 0.5), (255, 128, 0)], [0.3, 0.6]
        )
        numeric_values = (
            numeric_colors.tolist()
            if hasattr(numeric_colors, "tolist")
            else numeric_colors
        )
        self.assertEqual(
            numeric_values,
            [[0.1, 0.25, 0.5, 0.3], [1.0, 128 / 255, 0.0, 0.6]],
        )

        rgba_color = vector.generate_rgbas_array(
            SimpleNamespace(get_hex=lambda: "#0000FF"), 0.7
        )
        rgba_values = (
            rgba_color.tolist() if hasattr(rgba_color, "tolist") else rgba_color
        )
        self.assertEqual(rgba_values, [[0.0, 0.0, 1.0, 0.7]])

        compatible_color = vector.generate_rgbas_array(
            SimpleNamespace(to_hex=lambda: "#123456"), 0.9
        )
        compatible_values = (
            compatible_color.tolist()
            if hasattr(compatible_color, "tolist")
            else compatible_color
        )
        self.assertEqual(
            compatible_values,
            [[18 / 255, 52 / 255, 86 / 255, 0.9]],
        )

        shorthand_hex = vector.generate_rgbas_array("#F08", 0.25)
        shorthand_values = (
            shorthand_hex.tolist()
            if hasattr(shorthand_hex, "tolist")
            else shorthand_hex
        )
        self.assertEqual(
            shorthand_values,
            [[1.0, 0.0, 136 / 255, 0.25]],
        )

        real_import = builtins.__import__

        def without_numpy(name, *args, **kwargs):
            if name == "numpy":
                raise ImportError("NumPy intentionally unavailable in this test")
            return real_import(name, *args, **kwargs)

        with patch("builtins.__import__", side_effect=without_numpy):
            fallback = vector.generate_rgbas_array(compat._base.GREEN, 0.4)
        self.assertEqual(
            fallback,
            [[compat._base.GREEN.red, compat._base.GREEN.green,
              compat._base.GREEN.blue, 0.4]],
        )

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
        with patch.object(handles, "_group_set_stroke", return_value=group) as set_stroke:
            self.assertIs(group.set_stroke(compat._base.BLUE, 12, 0.4, family=False), group)
        set_stroke.assert_called_once_with(group, compat._base.BLUE, 12, 0.4, False)
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
        self.assertEqual(list(inspect.signature(compat.VGroup.set_style).parameters), [
            "self", "fill_color", "fill_opacity", "stroke_color", "stroke_width",
            "stroke_opacity", "background_stroke_color", "background_stroke_width",
            "background_stroke_opacity", "sheen_factor", "sheen_direction",
            "background_image", "family",
        ])
        self.assertIs(compat.VGroup.get_style, compat.VMobject.get_style)
        self.assertIs(compat.VGroup.cap_style, compat.VMobject.cap_style)
        self.assertIs(compat.VGroup.get_sheen_factor, compat.VMobject.get_sheen_factor)
        self.assertIs(compat.VGroup.get_sheen_direction, compat.VMobject.get_sheen_direction)
        self.assertIs(compat.VGroup.set_style, compat.VMobject.set_style)
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
        sheen_target = SimpleNamespace(setSheenDirection=Mock())
        with patch.object(handles, "_style_target", return_value=(sheen_target, None, "")):
            self.assertIs(obj.set_sheen(0.0, compat._base.UL), obj)
            self.assertIs(obj.set_sheen_direction(compat._base.UL), obj)
        self.assertEqual(sheen_target.setSheenDirection.call_count, 2)
        self.assertIs(obj.rotate_sheen_direction(0.0), obj)
        group = identity_only_wrapper(compat.VGroup)
        self.assertIs(group.rotate_sheen_direction(0.0), group)
        self.assertIsNone(group.get_background_image())

    def test_vmobject_init_colors_uses_supported_shared_paint(self):
        self.assertEqual(
            list(inspect.signature(compat.VMobject.init_colors).parameters),
            ["self", "propagate_colors"],
        )
        self.assertIs(compat.VGroup.init_colors, compat.VMobject.init_colors)

        vector = identity_only_wrapper(compat.VMobject)
        black = compat._base.BLACK
        with (
            patch.object(vector, "get_stroke_color", return_value=black),
            patch.object(vector, "get_stroke_width", side_effect=lambda background=False: 0 if background else 4),
            patch.object(vector, "get_stroke_opacity", side_effect=lambda background=False: 1 if background else 0.7),
            patch.object(vector, "get_sheen_factor", return_value=0),
            patch.object(vector, "get_background_image", return_value=None),
            patch.object(vector, "get_fill_color", return_value=compat._base.RED),
            patch.object(vector, "get_fill_opacity", return_value=0.6),
            patch.object(vector, "get_sheen_direction", return_value=[-1, 1, 0]),
            patch.object(vector, "set_fill") as set_fill,
            patch.object(vector, "set_stroke") as set_stroke,
            patch.object(vector, "set_background_stroke") as set_background_stroke,
            patch.object(vector, "set_sheen") as set_sheen,
        ):
            self.assertIs(vector.init_colors(propagate_colors=False), vector)
            set_fill.assert_called_once_with(
                color=compat._base.RED, opacity=0.6, family=False
            )
            set_stroke.assert_called_once_with(
                color=black, width=4, opacity=0.7, family=False
            )
            set_background_stroke.assert_called_once_with(
                color=black, width=0, opacity=1, family=False
            )
            set_sheen.assert_called_once_with(
                factor=0, direction=[-1, 1, 0], family=False
            )
            set_fill.reset_mock()
            set_stroke.reset_mock()
            set_background_stroke.reset_mock()
            set_sheen.reset_mock()
            self.assertIs(vector.init_colors(), vector)
            set_fill.assert_called_once_with(
                color=compat._base.RED, opacity=0.6, family=True
            )
            set_stroke.assert_called_once_with(
                color=black, width=4, opacity=0.7, family=True
            )
            set_background_stroke.assert_called_once_with(
                color=black, width=0, opacity=1, family=True
            )
            set_sheen.assert_called_once_with(
                factor=0, direction=[-1, 1, 0], family=True
            )

        with (
            patch.object(vector, "get_stroke_color", side_effect=lambda background=False: compat._base.BLUE if background else black),
            patch.object(vector, "get_stroke_width", side_effect=lambda background=False: 2 if background else 4),
            patch.object(vector, "get_stroke_opacity", side_effect=lambda background=False: 0.5 if background else 0.7),
            patch.object(vector, "get_sheen_factor", return_value=0),
            patch.object(vector, "get_background_image", return_value=None),
            patch.object(vector, "set_fill") as set_fill,
        ):
            with self.assertRaisesRegex(NotImplementedError, "background-stroke paint"):
                vector.init_colors()
            set_fill.assert_not_called()

    def test_update_rgbas_array_routes_solid_channels_to_shared_paint(self):
        self.assertEqual(
            list(inspect.signature(compat.VMobject.update_rgbas_array).parameters),
            ["self", "array_name", "color", "opacity"],
        )
        self.assertIs(
            compat.VGroup.update_rgbas_array,
            compat.VMobject.update_rgbas_array,
        )
        vector = identity_only_wrapper(compat.VMobject)
        alpha_color = compat._base.Color(0.1, 0.2, 0.3, 0.4)
        rgb_color = compat._base.Color(0.1, 0.2, 0.3, 1.0)
        with (
            patch.object(vector, "set_fill", return_value=vector) as set_fill,
            patch.object(vector, "set_stroke", return_value=vector) as set_stroke,
        ):
            self.assertIs(
                vector.update_rgbas_array("fill_rgbas", alpha_color, [0.7]),
                vector,
            )
            set_fill.assert_called_once_with(
                color=rgb_color, opacity=0.7, family=False
            )
            self.assertIs(
                vector.update_rgbas_array("stroke_rgbas", "#FF8000", None),
                vector,
            )
            self.assertEqual(set_stroke.call_args.kwargs["family"], False)
            self.assertEqual(set_stroke.call_args.kwargs["background"], False)
            self.assertEqual(set_stroke.call_args.kwargs["opacity"], None)
            self.assertIs(
                vector.update_rgbas_array(
                    "background_stroke_rgbas", compat._base.BLACK, 1.0
                ),
                vector,
            )
            self.assertEqual(set_stroke.call_args.kwargs["background"], True)

        with patch.object(vector, "set_fill") as set_fill:
            with self.assertRaisesRegex(NotImplementedError, "multi-color RGBA"):
                vector.update_rgbas_array("fill_rgbas", ["#FF0000", "#0000FF"])
            with self.assertRaisesRegex(NotImplementedError, "unsupported RGBA array"):
                vector.update_rgbas_array("custom_rgbas", "#FF0000")
            set_fill.assert_not_called()

        vector.sheen_factor = 0.25
        with self.assertRaisesRegex(NotImplementedError, "shared gradient paint"):
            vector.update_rgbas_array("fill_rgbas", "#FF0000")
        del vector.sheen_factor
        with self.assertRaisesRegex(NotImplementedError, "background strokes"):
            vector.update_rgbas_array("background_stroke_rgbas", compat._base.RED)

    def test_sheen_direction_mutators_use_shared_rust_detached_and_live_routes(self):
        obj = identity_only_wrapper(compat.VMobject)
        detached = SimpleNamespace(
            setSheenDirection=Mock(),
            rotateSheenDirection=Mock(),
            sheenDirection=Mock(return_value=[0.25, -0.5]),
        )
        with patch.object(handles, "_style_target", return_value=(detached, None, "")):
            self.assertIs(obj.set_sheen_direction([0, 1]), obj)
            detached.setSheenDirection.assert_called_once_with(0.0, 1.0, 0.0, 2, True)
            self.assertIs(obj.set_sheen_direction((1, 2, 3), family=False), obj)
            detached.setSheenDirection.assert_called_with(1.0, 2.0, 3.0, 3, False)
            self.assertIs(obj.rotate_sheen_direction(0.5, axis=(0, 0, 1)), obj)
            detached.rotateSheenDirection.assert_called_once_with(0.5, 0.0, 0.0, 1.0, True)
            direction = obj.get_sheen_direction()
        self.assertEqual(direction.tolist() if hasattr(direction, "tolist") else direction,
                         [0.25, -0.5])

        group = identity_only_wrapper(compat.VGroup)
        family = SimpleNamespace(
            setSheenDirection=Mock(),
            rotateSheenDirection=Mock(),
            ownSheenDirection=Mock(return_value=[1.0, 0.0, 0.0]),
        )
        with patch.object(handles, "_style_target", return_value=(family, None, "Family")):
            self.assertIs(group.set_sheen(0.0, compat._base.DOWN, family=False), group)
            family.setSheenDirection.assert_called_once_with(0.0, -1.0, 0.0, 2, False)
            self.assertIs(group.rotate_sheen_direction(1.0, axis=(0, 1), family=True), group)
            family.rotateSheenDirection.assert_called_once_with(1.0, 0.0, 1.0, 0.0, True)
            self.assertEqual(list(group.get_sheen_direction()), [1.0, 0.0, 0.0])

        live = SimpleNamespace(liveSetMobjectSheenDirection=Mock(),
                               liveRotateMobjectSheenDirection=Mock())
        with patch.object(handles, "_style_target", return_value=(detached, live, "")):
            self.assertIs(obj.set_sheen_direction(compat._base.RIGHT), obj)
            self.assertIs(obj.rotate_sheen_direction(0.25), obj)
        live.liveSetMobjectSheenDirection.assert_called_once_with(
            detached, 1.0, 0.0, 0.0, 2, True
        )
        live.liveRotateMobjectSheenDirection.assert_called_once_with(
            detached, 0.25, 0.0, 0.0, 1.0, True
        )

        family_live = SimpleNamespace(liveSetFamilySheenDirection=Mock(),
                                      liveRotateFamilySheenDirection=Mock())
        with patch.object(handles, "_style_target", return_value=(family, family_live, "Family")):
            self.assertIs(group.set_sheen_direction(compat._base.UP, family=False), group)
            self.assertIs(group.rotate_sheen_direction(-0.5, family=False), group)
        family_live.liveSetFamilySheenDirection.assert_called_once_with(
            family, 0.0, 1.0, 0.0, 2, False
        )
        family_live.liveRotateFamilySheenDirection.assert_called_once_with(
            family, -0.5, 0.0, 0.0, 1.0, False
        )

    def test_sheen_direction_staged_writes_and_invalid_values_do_not_dispatch(self):
        obj = identity_only_wrapper(compat.VMobject)
        target = SimpleNamespace(setSheenDirection=Mock(), rotateSheenDirection=Mock())
        with patch.object(handles, "_style_target", return_value=(target, None, "")):
            with self.assertRaises(ValueError):
                obj.set_sheen_direction((float("nan"), 0.0, 0.0))
            with self.assertRaises(ValueError):
                obj.rotate_sheen_direction(float("inf"))
            with patch.object(handles, "_check_sheen_callback_write",
                              side_effect=NotImplementedError("staged")):
                with self.assertRaises(NotImplementedError):
                    obj.set_sheen_direction(compat._base.UP)
                with self.assertRaises(NotImplementedError):
                    obj.rotate_sheen_direction(0.5)
        target.setSheenDirection.assert_not_called()
        target.rotateSheenDirection.assert_not_called()

    def test_vmobject_constructor_sheen_direction_uses_typed_geometry_options(self):
        options = SimpleNamespace(setSheenDirection=Mock())
        handles._apply_shared_constructor_options(
            options,
            {"sheen_direction": (0.25, -0.5), "sheen_factor": 0.0},
            allow_sheen_direction=True,
        )
        options.setSheenDirection.assert_called_once_with(0.25, -0.5, 0.0, 2)

        options.setSheenDirection.reset_mock()
        with self.assertRaisesRegex(TypeError, "sheen_direction"):
            handles._apply_shared_constructor_options(
                options, {"sheen_direction": (0.25, -0.5, 1.0)}
            )
        with self.assertRaises(ValueError):
            handles._apply_shared_constructor_options(
                options,
                {"sheen_direction": (float("nan"), 0.0, 0.0)},
                allow_sheen_direction=True,
            )
        with self.assertRaises(NotImplementedError):
            handles._apply_shared_constructor_options(
                options, {"sheen_factor": 0.2}, allow_sheen_direction=True
            )
        options.setSheenDirection.assert_not_called()

    def test_cap_style_constructor_and_mutations_use_shared_rust_state(self):
        cap = compat._base.CapStyleType
        constructor_handle = SimpleNamespace(setStrokeCap=Mock())
        handles._apply_shared_constructor_options(
            constructor_handle, {"cap_style": cap.SQUARE}
        )
        constructor_handle.setStrokeCap.assert_called_once_with("square")
        with self.assertRaises(TypeError):
            handles._apply_shared_constructor_options(
                constructor_handle, {"cap_style": cap.ROUND, "stroke_cap": "butt"}
            )

        obj = identity_only_wrapper(compat.VMobject)
        detached = SimpleNamespace(setStrokeCap=Mock(), strokeCap="square")
        with patch.object(handles, "_style_target", return_value=(detached, None, "")):
            self.assertIs(obj.set_cap_style(cap.SQUARE), obj)
            self.assertIs(obj.cap_style, cap.SQUARE)
            obj.cap_style = cap.ROUND
        self.assertEqual(
            [call.args for call in detached.setStrokeCap.call_args_list],
            [("square",), ("round",)],
        )

        live_context = SimpleNamespace(
            liveSetStrokeCap=Mock(), queryMobjectStrokeCap=Mock(return_value="butt")
        )
        with patch.object(handles, "_style_target", return_value=(detached, live_context, "")):
            self.assertIs(obj.set_cap_style(cap.BUTT), obj)
            self.assertIs(obj.cap_style, cap.BUTT)
        live_context.liveSetStrokeCap.assert_called_once_with(detached, "butt")
        live_context.queryMobjectStrokeCap.assert_called_once_with(detached)

        family = identity_only_wrapper(compat.VGroup)
        family_handle = SimpleNamespace(
            setStrokeCap=Mock(), ownStrokeCap=Mock(return_value="butt")
        )
        with patch.object(handles, "_style_target", return_value=(family_handle, None, "Family")):
            self.assertIs(family.set_cap_style(cap.BUTT), family)
            self.assertIs(family.cap_style, cap.BUTT)
        family_handle.setStrokeCap.assert_called_once_with("butt")

        family_context = SimpleNamespace(liveSetFamilyOwnStrokeCap=Mock())
        with patch.object(
            handles, "_style_target", return_value=(family_handle, family_context, "Family")
        ):
            self.assertIs(family.set_cap_style(cap.ROUND), family)
        family_context.liveSetFamilyOwnStrokeCap.assert_called_once_with(family_handle, "round")

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
        style = {
            "fill_color": compat._base.GREEN,
            "fill_opacity": 0.3,
            "stroke_color": compat._base.BLUE,
            "stroke_width": 4.0,
            "stroke_opacity": 0.45,
        }
        with patch.object(compat.VGroup, "get_style", return_value=style) as get_style, \
             patch.object(compat.VGroup, "set_style", return_value=source) as set_style:
            self.assertIs(source.match_style(target, family=False), source)
        get_style.assert_called_once_with(simple=True)
        set_style.assert_called_once_with(**style, family=False)
        with self.assertRaises(NotImplementedError):
            identity_only_wrapper(compat.Group).match_style(target, family=False)
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
