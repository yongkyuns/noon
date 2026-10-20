"""Frontend coercion/dispatch; Rust tests own family paint semantics and atomicity."""
import unittest
from unittest.mock import Mock, patch

import _manim_compat as compat

import _manim_semantic_handles as handles
import _manim_updaters as updaters
from _typed_geometry_test_support import identity_only_wrapper


class SharedFamilyPaintTests(unittest.TestCase):
    def test_generic_group_opacity_is_deprecated_metadata_without_paint_dispatch(self):
        group = identity_only_wrapper(compat.Group, submobjects=[])
        group._semantic_family_handle = Mock()
        value = object()
        with self.assertWarnsRegex(DeprecationWarning, 'Mobject.set'):
            self.assertIs(group.set_opacity(value=value), group)
        self.assertIs(group.opacity, value)
        group._semantic_family_handle.setOpacity.assert_not_called()
        with self.assertRaises(TypeError):
            group.set_opacity(opacity=.5)

    def test_unbacked_generic_group_metadata_does_not_require_a_rust_handle(self):
        group = identity_only_wrapper(compat.Group, submobjects=[])
        with self.assertWarns(DeprecationWarning):
            group.set_opacity(None)
        self.assertIsNone(group.opacity)


    def test_constructor_color_is_passed_in_the_single_family_creation(self):
        handle = Mock()
        for context in (None, Mock()):
            with patch.object(handles, "_create_family_handle", return_value=handle) as create, \
                 patch.object(handles, "_new_membership_batch", return_value=Mock()), \
                 patch.object(handles, "_live_constructor_context", return_value=context):
                group = compat.Group(color=compat._base.GREEN)
            expected = (compat._base.GREEN.red, compat._base.GREEN.green,
                        compat._base.GREEN.blue, compat._base.GREEN.alpha)
            if context is None:
                self.assertEqual(create.call_args.args[1:], (0.0, *expected))
                handle.setOwnColor.assert_not_called()
            else:
                context.liveCreateFamily.assert_not_called()
                self.assertEqual(context.liveCreateFamilyWithColor.call_args.args[1:], (0.0, *expected))

    def test_omitted_constructor_color_is_white_but_explicit_none_is_black(self):
        with patch.object(handles, "_create_family_handle") as create, \
             patch.object(handles, "_new_membership_batch", return_value=Mock()), \
             patch.object(handles, "_live_constructor_context", return_value=None):
            compat.Group()
            self.assertEqual(create.call_args.args[1:], (0.0,))
            compat.Group(color=None)
            self.assertEqual(create.call_args.args[1:], (0.0, 0.0, 0.0, 0.0, 1.0))

    def test_invalid_constructor_color_rejects_before_family_allocation(self):
        with patch.object(handles, "_create_family_handle") as create, \
             patch.object(handles, "_new_membership_batch") as batch:
            with self.assertRaises((TypeError, ValueError)):
                compat.Group(color=object())
        create.assert_not_called()
        batch.assert_not_called()

    def test_generic_group_fill_and_stroke_are_metadata_without_paint_dispatch(self):
        for name in ("fill", "stroke"):
            for backed in (False, True):
                group = identity_only_wrapper(compat.Group, submobjects=[])
                if backed:
                    group._semantic_family_handle = Mock()
                value = object()
                setter = getattr(group, "set_" + name)
                with self.assertWarnsRegex(DeprecationWarning, "Mobject.set"):
                    self.assertIs(setter(value=value), group)
                self.assertIs(getattr(group, name), value)
                with self.assertRaises(TypeError):
                    setter(color=compat._base.GREEN)
                if backed:
                    self.assertEqual(group._semantic_family_handle.method_calls, [])

    def test_existing_vector_wrappers_keep_shared_fill_and_stroke_dispatch(self):
        for cls in (compat.VGroup, compat._base.Arrow, compat._base.Graph,
                    compat._base.SampleSpace, compat._base.NumberLine,
                    compat._base.Axes, compat._base.ThreeDAxes,
                    compat._base.Cube, compat._base.Prism):
            with self.subTest(cls=cls):
                group = identity_only_wrapper(cls, submobjects=[])
                group._semantic_family_handle = Mock()
                with patch.object(handles, "_group_set_fill", return_value=group) as fill, \
                     patch.object(handles, "_group_set_stroke", return_value=group) as stroke, \
                     patch.object(handles, "_group_set_opacity", return_value=group) as opacity:
                    self.assertIs(group.set_fill(compat._base.GREEN, .75), group)
                    self.assertIs(group.set_stroke(compat._base.YELLOW, 25, .625), group)
                    self.assertIs(group.set_opacity(.5), group)
                fill.assert_called_once_with(group, compat._base.GREEN, .75, True)
                stroke.assert_called_once_with(group, compat._base.YELLOW, 25, .625, True)
                opacity.assert_called_once_with(group, .5, True)

    def test_root_only_color_uses_one_shared_root_operation(self):
        group = identity_only_wrapper(compat.Group, submobjects=[])
        group._semantic_family_handle = Mock()
        with patch.object(handles, "_group_target_context", return_value=None):
            self.assertIs(group.set_color(compat._base.BLUE, family=False), group)
        group._semantic_family_handle.setOwnColor.assert_called_once_with(
            compat._base.BLUE.red, compat._base.BLUE.green, compat._base.BLUE.blue, compat._base.BLUE.alpha)
        group._semantic_family_handle.setColor.assert_not_called()

    def test_root_only_fill_stroke_and_opacity_use_atomic_shared_style_operations(self):
        color = compat._base.GREEN
        stroke_color = compat._base.BLUE
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                with patch.object(handles, "_group_target_context", return_value=context):
                    self.assertIs(family.set_fill(color, 0.35, family=False), family)
                    self.assertIs(family.set_stroke(stroke_color, 4, 0.45, family=False), family)
                    self.assertIs(family.set_opacity(0.6, family=False), family)
                owner = context if live else family._semantic_family_handle
                fill = owner.liveSetFamilyOwnFill if live else owner.setOwnFill
                stroke = owner.liveSetFamilyOwnStroke if live else owner.setOwnStroke
                opacity = owner.liveSetFamilyOwnOpacity if live else owner.setOwnOpacity
                prefix = (family._semantic_family_handle,) if live else ()
                fill.assert_called_once_with(
                    *prefix, True, color.red, color.green, color.blue, color.alpha, 0.35
                )
                stroke.assert_called_once_with(
                    *prefix, True, stroke_color.red, stroke_color.green,
                    stroke_color.blue, stroke_color.alpha, 0.04, 0.45
                )
                opacity.assert_called_once_with(*prefix, 0.6)

    def test_root_only_set_style_publishes_one_authored_or_live_transaction(self):
        fill_color = compat._base.GREEN
        stroke_color = compat._base.BLUE
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                with patch.object(
                    handles,
                    "_style_target",
                    return_value=(family._semantic_family_handle, context, "Family"),
                ):
                    self.assertIs(
                        family.set_style(
                            fill_color=fill_color,
                            fill_opacity=0.35,
                            stroke_color=stroke_color,
                            stroke_width=4,
                            stroke_opacity=0.45,
                            family=False,
                        ),
                        family,
                    )
                owner = context if live else family._semantic_family_handle
                method = owner.liveSetFamilyOwnStyle if live else owner.setOwnStyle
                prefix = (family._semantic_family_handle,) if live else ()
                method.assert_called_once_with(
                    *prefix,
                    True,
                    fill_color.red,
                    fill_color.green,
                    fill_color.blue,
                    fill_color.alpha,
                    0.35,
                    True,
                    stroke_color.red,
                    stroke_color.green,
                    stroke_color.blue,
                    stroke_color.alpha,
                    0.04,
                    0.45,
                )

    def test_root_only_set_style_with_background_stroke_is_atomic_authored_or_live(self):
        fill_color = compat._base.GREEN
        stroke_color = compat._base.BLUE
        for live in (False, True):
            for background_opacity in (0.55, 1.0):
                with self.subTest(live=live, background_opacity=background_opacity):
                    family = identity_only_wrapper(compat.VGroup, submobjects=[])
                    family._semantic_family_handle = Mock()
                    context = Mock() if live else None
                    with patch.object(
                        handles,
                        "_style_target",
                        return_value=(family._semantic_family_handle, context, "Family"),
                    ):
                        self.assertIs(
                            family.set_style(
                                fill_color=fill_color,
                                fill_opacity=0.35,
                                stroke_color=stroke_color,
                                stroke_width=4,
                                stroke_opacity=0.45,
                                background_stroke_color=stroke_color,
                                background_stroke_width=6,
                                background_stroke_opacity=background_opacity,
                                family=False,
                            ),
                            family,
                        )
                    owner = context if live else family._semantic_family_handle
                    method = (
                        owner.liveSetFamilyOwnStyleWithBackgroundStroke
                        if live
                        else owner.setOwnStyleWithBackgroundStroke
                    )
                    prefix = (family._semantic_family_handle,) if live else ()
                    method.assert_called_once_with(
                        *prefix,
                        True,
                        fill_color.red,
                        fill_color.green,
                        fill_color.blue,
                        fill_color.alpha,
                        0.35,
                        True,
                        stroke_color.red,
                        stroke_color.green,
                        stroke_color.blue,
                        stroke_color.alpha,
                        0.04,
                        0.45,
                        True,
                        stroke_color.red,
                        stroke_color.green,
                        stroke_color.blue,
                        stroke_color.alpha,
                        0.06,
                        background_opacity,
                    )
                    if live:
                        context.liveSetFamilyOwnStyle.assert_not_called()
                        family._semantic_family_handle.setOwnStyle.assert_not_called()
                    else:
                        family._semantic_family_handle.setOwnStyle.assert_not_called()

    def test_vgroup_root_opacity_getters_read_the_family_style_slot(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock(
            ownFillColor=Mock(return_value=compat._base.GREEN),
            ownStrokeColor=Mock(return_value=compat._base.BLUE),
            ownStrokeWidth=Mock(return_value=0.04),
            ownFillOpacity=Mock(return_value=0.35),
            ownStrokeOpacity=Mock(return_value=0.6),
        )
        self.assertEqual(family.get_fill_color(), compat._base.GREEN)
        self.assertEqual(family.get_stroke_color(), compat._base.BLUE)
        self.assertEqual(family.get_stroke_width(), 4.0)
        fill_rows = family.get_fill_rgbas()
        stroke_rows = family.get_stroke_rgbas()
        fill_rows = fill_rows.tolist() if hasattr(fill_rows, "tolist") else fill_rows
        stroke_rows = stroke_rows.tolist() if hasattr(stroke_rows, "tolist") else stroke_rows
        self.assertEqual(fill_rows[0], [
            compat._base.GREEN.red, compat._base.GREEN.green,
            compat._base.GREEN.blue, 0.35,
        ])
        self.assertEqual(stroke_rows[0], [
            compat._base.BLUE.red, compat._base.BLUE.green,
            compat._base.BLUE.blue, 0.6,
        ])
        self.assertEqual(family.get_fill_opacity(), 0.35)
        self.assertEqual(family.get_stroke_opacity(), 0.6)
        style = family.get_style(simple=True)
        self.assertEqual(style["fill_color"], compat._base.GREEN)
        self.assertEqual(style["fill_opacity"], 0.35)
        self.assertEqual(style["stroke_color"], compat._base.BLUE)
        self.assertEqual(style["stroke_width"], 4.0)
        self.assertEqual(style["stroke_opacity"], 0.6)
        self.assertEqual(family._semantic_family_handle.ownFillColor.call_count, 3)
        self.assertEqual(family._semantic_family_handle.ownStrokeColor.call_count, 3)
        self.assertEqual(family._semantic_family_handle.ownStrokeWidth.call_count, 2)
        self.assertEqual(family._semantic_family_handle.ownFillOpacity.call_count, 5)
        self.assertEqual(family._semantic_family_handle.ownStrokeOpacity.call_count, 5)

    def test_vgroup_nonrecursive_fade_uses_one_authored_or_live_root_operation(self):
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                with patch.object(
                    handles,
                    "_style_target",
                    return_value=(family._semantic_family_handle, context, "Family"),
                ):
                    self.assertIs(handles._fade(family, 0.25, family=False), family)
                if live:
                    context.liveFadeFamilyOwn.assert_called_once_with(
                        family._semantic_family_handle, 0.25
                    )
                    family._semantic_family_handle.fadeOwn.assert_not_called()
                else:
                    family._semantic_family_handle.fadeOwn.assert_called_once_with(
                        0.25
                    )

    def test_vgroup_nonrecursive_fade_rejects_staged_callbacks_before_writes(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            with self.assertRaisesRegex(NotImplementedError, "staged callbacks"):
                family.fade(0.25, family=False)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_not_called()
        family._semantic_family_handle.fadeOwn.assert_not_called()

    def test_vgroup_background_stroke_query_reads_the_root_family_state(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        background_color = compat._base.BLUE
        family._semantic_family_handle = Mock(
            ownBackgroundStrokeColor=Mock(
                return_value=Mock(
                    red=background_color.red,
                    green=background_color.green,
                    blue=background_color.blue,
                    alpha=background_color.alpha,
                )
            ),
            ownBackgroundStrokeWidth=Mock(return_value=0.06),
            ownBackgroundStrokeOpacity=Mock(return_value=0.75),
        )
        self.assertEqual(family.get_stroke_opacity(background=True), 0.75)
        self.assertEqual(family.get_stroke_width(background=True), 6.0)
        self.assertEqual(family.get_stroke_color(background=True), compat._base.BLUE)
        rows = family.get_stroke_rgbas(background=True)
        rows = rows.tolist() if hasattr(rows, "tolist") else rows
        self.assertEqual(
            rows[0],
            [background_color.red, background_color.green, background_color.blue, 0.75],
        )

    def test_vgroup_background_stroke_setter_routes_to_root_only_owner(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        context = Mock()
        with patch.object(handles, "_group_target_context", return_value=context):
            self.assertIs(family.set_background_stroke(opacity=0.5, family=False), family)
        context.liveSetFamilyOwnStyleWithBackgroundStroke.assert_called_once_with(
            family._semantic_family_handle,
            False, 0.0, 0.0, 0.0, 1.0, None,
            False, 0.0, 0.0, 0.0, 1.0, None, None,
            False, 0.0, 0.0, 0.0, 1.0,
            None, 0.5,
        )

    def test_root_only_callback_color_fails_before_leaf_mutation(self):
        group = identity_only_wrapper(compat.Group, submobjects=[])
        group._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            with self.assertRaisesRegex(NotImplementedError, "metadata writes"):
                group.set_color(compat._base.BLUE, family=False)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_not_called()

    def test_root_only_callback_fill_rejects_without_staging_descendant_paint(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            with self.assertRaisesRegex(NotImplementedError, "staged family style"):
                family.set_fill(compat._base.BLUE, 0.4, family=False)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_not_called()

    def test_root_only_callback_stroke_rejects_without_staging_descendant_paint(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            with self.assertRaisesRegex(NotImplementedError, "staged family style"):
                family.set_stroke(compat._base.BLUE, 4, 0.4, family=False)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_not_called()


    def test_leaf_fade_forwards_default_and_live_calls_without_python_alpha_math(self):
        for live in (False, True):
            leaf = identity_only_wrapper(compat.Square)
            handle = Mock()
            context = Mock() if live else None
            with patch.object(handles, "_handle_for", return_value=handle), \
                 patch.object(handles, "_live_mutation_context", return_value=context):
                self.assertIs(leaf.fade(family=False), leaf)
            if live:
                context.liveFade.assert_called_once_with(handle, 0.5)
                handle.fade.assert_not_called()
            else:
                handle.fade.assert_called_once_with(0.5)

    def test_plain_group_nonrecursive_fade_is_a_noop_and_vector_group_fades_its_root(self):
        leaf = identity_only_wrapper(compat.Square)
        plain_group = identity_only_wrapper(compat.Group, submobjects=[])
        plain_group._semantic_family_handle = Mock()
        group = identity_only_wrapper(compat.VGroup, submobjects=[])
        group._semantic_family_handle = Mock()
        with patch.object(handles, "_handle_for") as get_handle:
            for darkness in (-0.1, 1.1, float("nan"), float("inf")):
                with self.assertRaises(ValueError):
                    leaf.fade(darkness)
            get_handle.assert_not_called()
        self.assertIs(plain_group.fade(float("nan"), family=False), plain_group)
        plain_group._semantic_family_handle.assert_not_called()
        with patch.object(
            handles,
            "_style_target",
            return_value=(group._semantic_family_handle, None, "Family"),
        ):
            self.assertIs(group.fade(0.25, family=False), group)
        group._semantic_family_handle.fadeOwn.assert_called_once_with(0.25)
        group._semantic_family_handle.fade.assert_not_called()

    def test_plain_group_nonrecursive_fade_is_a_noop_in_callback_phase(self):
        group = identity_only_wrapper(compat.Group, submobjects=[])
        group._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            self.assertIs(group.fade(float("nan"), family=False), group)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_not_called()

    def test_leaf_fade_to_forwards_rgba_and_unbounded_finite_interpolation(self):
        leaf = identity_only_wrapper(compat.Square)
        color = compat._base.Color(0.2, 0.4, 0.6, 0.8)
        for live in (False, True):
            with self.subTest(live=live):
                handle = Mock()
                context = Mock() if live else None
                with patch.object(handles, "_handle_for", return_value=handle), \
                     patch.object(handles, "_live_mutation_context", return_value=context):
                    self.assertIs(leaf.fade_to(color, alpha=1.25), leaf)
                if live:
                    context.liveFadeTo.assert_called_once_with(
                        handle, color.red, color.green, color.blue, color.alpha, 1.25
                    )
                    handle.fadeTo.assert_not_called()
                else:
                    handle.fadeTo.assert_called_once_with(
                        color.red, color.green, color.blue, color.alpha, 1.25
                    )

    def test_invalid_fade_to_amount_fails_before_host_mutation(self):
        leaf = identity_only_wrapper(compat.Square)
        with patch.object(handles, "_handle_for") as get_handle:
            with self.assertRaises(TypeError):
                leaf.fade_to(compat._base.BLUE)
            for amount in (float("nan"), float("inf"), -float("inf")):
                with self.assertRaises(ValueError):
                    leaf.fade_to(compat._base.BLUE, amount)
            get_handle.assert_not_called()

    def test_group_fade_to_routes_once_through_family_owner(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        target = compat._base.Color(0.1, 0.2, 0.3, 0.4)
        for live in (False, True):
            context = Mock() if live else None
            with patch.object(handles, "_group_target_context", return_value=context):
                self.assertIs(family.fade_to(target, 0.75), family)
            owner = context if live else family._semantic_family_handle
            method = owner.liveSetFamilyFadeTo if live else owner.fadeTo
            expected = ((family._semantic_family_handle,) if live else ()) + (
                target.red, target.green, target.blue, target.alpha, 0.75
            )
            method.assert_called_once_with(*expected)

    def test_nonrecursive_group_fade_to_is_a_root_only_noop(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        with patch.object(handles, "_fade_to") as fade_to:
            self.assertIs(family.fade_to(compat._base.BLUE, 0.5, family=False), family)
        fade_to.assert_not_called()
        self.assertEqual(family._semantic_family_handle.method_calls, [])

    def test_callback_leaf_and_family_fade_to_use_staged_operations(self):
        from types import SimpleNamespace
        from test_updater_snapshot import CanonicalCallbackPropertyRowTests
        _, leaf, context = CanonicalCallbackPropertyRowTests._mobject_and_context()
        _, row = context.row(leaf)
        previous = row.style
        result = SimpleNamespace(hasFill=True, fillRed=0.25, fillGreen=0.5,
            fillBlue=0.75, fillAlpha=0.8, hasStroke=False)
        context._operations.callbackPaintFadeTo = Mock(return_value=result)
        color = compat._base.Color(0.2, 0.4, 0.6, 0.8)
        with patch.object(updaters, "_canonical_phase_context", return_value=context):
            self.assertIs(leaf.fade_to(color, 0.5), leaf)
        context._operations.callbackPaintFadeTo.assert_called_once_with(
            *updaters._phase_optional_color_args(previous.fill),
            *updaters._phase_optional_color_args(previous.stroke),
            color.red, color.green, color.blue, color.alpha, 0.5
        )
        self.assertEqual(row.style.fill, (0.25, 0.5, 0.75, 0.8))

        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            self.assertIs(family.fade_to(color, 0.5), family)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_called_once_with(
            family._semantic_family_handle,
            "FadeTo",
            (color.red, color.green, color.blue, color.alpha, 0.5),
        )

    def test_image_fade_cannot_inherit_vector_alpha_multiplication(self):
        from _manim_image import ImageMobject
        image = identity_only_wrapper(ImageMobject)
        with patch.object(handles, "_handle_for") as get_handle:
            with self.assertRaisesRegex(NotImplementedError, "image alpha reset"):
                image.fade()
            get_handle.assert_not_called()

    def test_image_fade_to_is_a_noop_like_pinned_manim(self):
        from _manim_image import ImageMobject
        image = identity_only_wrapper(ImageMobject)
        with patch.object(handles, "_handle_for") as get_handle:
            self.assertIs(image.fade_to(compat._base.BLUE, 0.5), image)
            get_handle.assert_not_called()

    def test_callback_leaf_fade_uses_preceding_style_and_installs_shared_result(self):
        from types import SimpleNamespace
        from test_updater_snapshot import CanonicalCallbackPropertyRowTests
        _, leaf, context = CanonicalCallbackPropertyRowTests._mobject_and_context()
        _, row = context.row(leaf)
        previous = row.style
        result = SimpleNamespace(hasFill=True, fillRed=0.1, fillGreen=0.2,
            fillBlue=0.3, fillAlpha=0.123, hasStroke=False)
        context._operations.callbackPaintFade = Mock(return_value=result)
        with patch.object(updaters, "_canonical_phase_context", return_value=context):
            self.assertIs(leaf.fade(0.25), leaf)
        context._operations.callbackPaintFade.assert_called_once_with(
            *updaters._phase_optional_color_args(previous.fill),
            *updaters._phase_optional_color_args(previous.stroke), 0.25)
        self.assertEqual(row.style.fill, (0.1, 0.2, 0.3, 0.123))
        self.assertIsNone(row.style.stroke)
        self.assertEqual(row.style.opacity, previous.opacity)

    def test_callback_group_fade_is_one_shared_family_operation(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            self.assertIs(family.fade(0.25), family)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        phase.paint_family.assert_called_once_with(family._semantic_family_handle, "Fade", (0.25,))
        family._semantic_family_handle.fade.assert_not_called()

    def test_authored_and_live_calls_forward_one_family_handle(self):
        for live in (False, True):
            with self.subTest(live=live):
                family = identity_only_wrapper(compat.VGroup, submobjects=[])
                family._semantic_family_handle = Mock()
                context = Mock() if live else None
                with patch.object(handles, "_group_target_context", return_value=context):
                    self.assertIs(family.set_color(compat._base.RED), family)
                    self.assertIs(family.set_fill(compat._base.BLUE, 0.4), family)
                    self.assertIs(family.set_stroke(compat._base.RED, 2, 0.25), family)
                    self.assertIs(family.set_opacity(0.5), family)
                    self.assertIs(family.fade(0.25), family)
                owner = context if live else family._semantic_family_handle
                prefix = (family._semantic_family_handle,) if live else ()
                def method(name):
                    return getattr(owner, f"liveSetFamily{name}" if live else f"set{name}")
                color = compat._base.RED
                method("Color").assert_called_once_with(*prefix, color.red, color.green, color.blue, color.alpha)
                color = compat._base.BLUE
                method("Fill").assert_called_once_with(*prefix, True, color.red, color.green, color.blue, color.alpha, 0.4)
                color = compat._base.RED
                method("Stroke").assert_called_once_with(*prefix, True, color.red, color.green, color.blue, color.alpha, compat._manim_stroke_width(2), 0.25)
                method("Opacity").assert_called_once_with(*prefix, 0.5)
                if live:
                    context.liveSetFamilyFade.assert_called_once_with(
                        family._semantic_family_handle, 0.25
                    )
                    family._semantic_family_handle.fade.assert_not_called()
                else:
                    family._semantic_family_handle.fade.assert_called_once_with(0.25)

    def test_callback_paint_keeps_using_effective_overlay(self):
        member = identity_only_wrapper(compat.Square)
        member.set_fill = Mock()
        family = identity_only_wrapper(compat.VGroup, submobjects=[member])
        family._semantic_family_handle = Mock()
        phase = Mock()
        token = updaters._ACTIVE_CANONICAL_CONTEXT.set(phase)
        try:
            family.set_fill(compat._base.RED, 0.5)
        finally:
            updaters._ACTIVE_CANONICAL_CONTEXT.reset(token)
        color = compat._base.RED
        phase.paint_family.assert_called_once_with(family._semantic_family_handle, "Fill",
            (True, color.red, color.green, color.blue, color.alpha, 0.5))
        member.set_fill.assert_not_called()
        family._semantic_family_handle.setFill.assert_not_called()

    def test_unbacked_family_cannot_mutate(self):
        family = identity_only_wrapper(compat.VGroup, submobjects=[])
        with self.assertRaisesRegex(RuntimeError, "shared Rust"):
            family.set_opacity(0.5)


class FamilyCallbackBatchTests(unittest.TestCase):
    def fixture(self):
        import copy
        from types import SimpleNamespace
        from test_updater_snapshot import CanonicalCallbackPropertyRowTests
        _, mobject, context = CanonicalCallbackPropertyRowTests._mobject_and_context()
        context.token["publication"]["scene_revision"] = "9"
        _, row = context.row(mobject)
        second = copy.deepcopy(context._frame_items[(11, 3)])
        second["node"] = {"slot": 12, "generation": 3}
        context._operations.callbackFamilyKeys = Mock(return_value=["11:3", "12:3"])
        context._operations.callbackFamilyPaint = Mock()
        family = SimpleNamespace(semanticSlot=21, semanticGeneration=3)
        return context, row, second, family

    def test_sparse_family_is_one_read_and_preserves_preceding_overlay_writes(self):
        import json
        from dataclasses import replace
        context, row, second, family = self.fixture()
        row.style = replace(row.style, opacity=0.25)
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        # No changes returned by the Rust operation: verify adaptation, not paint math.
        context._operations.callbackFamilyPaint.return_value = "[]"
        context.paint_family(family, "Opacity", (0.5,))
        context._read.assert_called_once_with("family", (21, 3))
        forwarded = json.loads(context._operations.callbackFamilyPaint.call_args.args[3])
        self.assertEqual(forwarded[0][2]["opacity"], 0.25)
        self.assertEqual(context.effective_batch()["writes"], [])
        context._read.reset_mock()
        context.paint_family(family, "Opacity", (0.5,))
        context._read.assert_not_called()

    def test_failed_bulk_read_or_late_result_decode_cannot_publish_partial_edits(self):
        import json
        context, row, second, family = self.fixture()
        before = row.style
        context._read = Mock(side_effect=RuntimeError("last member is not live"))
        with self.assertRaisesRegex(RuntimeError, "last member"):
            context.paint_family(family, "Opacity", (0.5,))
        context._operations.callbackFamilyPaint.assert_not_called()
        self.assertEqual(row.style, before)
        self.assertEqual(context.effective_batch()["writes"], [])
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        changed = {**before.to_wire(), "opacity": 0.5}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, changed], [12, 3, {}]])
        with self.assertRaises(TypeError):
            context.paint_family(family, "Opacity", (0.5,))
        self.assertEqual(row.style, before)
        self.assertEqual(context.effective_batch()["writes"], [])
        self.assertNotIn((12, 3), context._rows)


    def test_family_recolor_preserves_bounds_but_stroke_width_invalidates_them(self):
        import json
        context, row, second, family = self.fixture()
        context._read = Mock(return_value={"kind": "family", "objects": [context._frame_items[(11, 3)], second]})
        bounds = row.require_bounds()
        recolored = row.style.to_wire()
        recolored["stroke"] = {"red": 0.2, "green": 0.4, "blue": 1.0, "alpha": 0.6}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, recolored]])
        context.paint_family(family, "Color", (0.2, 0.4, 1.0, 0.6))
        self.assertEqual(row.require_bounds(), bounds)
        wider = {**recolored, "stroke_width": 3.0}
        context._operations.callbackFamilyPaint.return_value = json.dumps([[11, 3, wider]])
        context.paint_family(family, "Stroke", (False, 0, 0, 0, 1, 3.0, None))
        with self.assertRaisesRegex(NotImplementedError, "spatial property change"):
            row.require_bounds()
