"""Argument/ownership tests only; shared Rust and Pyodide qualify semantics."""
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import _manim_number_labels as labels
import _manim_plotting as plotting


class Owned:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)
        self.freed = 0
    def free(self):
        self.freed += 1
    def setDirection(self, *value):
        self.direction = value
    def setExcludeZero(self, value):
        self.exclude = value
    def setIncludeSign(self, value):
        self.include_sign = value
    def setGroupWithCommas(self, value):
        self.group_with_commas = value
    def setShowEllipsis(self, value):
        self.show_ellipsis = value
    def setUnit(self, value):
        self.unit = value
    def setLogBase(self, value):
        self.log_base = value
    def setExcludedNumbers(self, value):
        self.excluded = value
    def setTickRangeFlags(self, include_tip, exclude_origin_tick):
        self.tick_range_flags = (include_tip, exclude_origin_tick)
    def setColor(self, *value):
        self.color = value


class LabelFamilyTypeTests(unittest.TestCase):
    def test_native_labels_keep_vector_style_operations(self):
        handle = SimpleNamespace(numberLabelMembers=lambda: [])
        with patch.object(labels, 'engine_call', side_effect=lambda fn, *args: fn(*args)):
            family = labels._family(handle, 18, labels._base.WHITE)
        self.assertIsInstance(family, labels._compat.VGroup)
        with patch.object(labels._shared, '_group_set_stroke', return_value=family) as stroke:
            self.assertIs(family.set_stroke(width=2), family)
        stroke.assert_called_once_with(family, None, 2, None)


class NumericLabelAdapterTests(unittest.TestCase):
    def setUp(self):
        self.options = []
        self.created = []
        self.calls = []
        self.addCleanup(patch.stopall)
        patch.object(labels._plot, '_coordinate_options', SimpleNamespace(numberLabels=self.make)).start()
        patch.object(labels._plot, '_to_js', side_effect=list).start()
        patch.object(labels._plot, '_outside_callback').start()
        patch.object(labels._shared, '_live_constructor_context', return_value=None).start()
        patch.object(labels, 'engine_call', side_effect=lambda fn, *args: fn(*args)).start()
        self.wrapped = patch.object(labels, '_family', side_effect=lambda handle, *args: handle).start()
        patch.object(labels._shared, '_family_wrapper_key', side_effect=id).start()
        class VectorGroupFixture(labels._compat.VGroup):
            def __new__(cls, *families):
                return families
        patch.object(labels._compat, 'VGroup', VectorGroupFixture).start()
        self.axis = SimpleNamespace(_semantic_family_handle=SimpleNamespace(
                                        numberLabelFamily=self.commit,
                                        numberLineDecimalNumberLabelFamily=self.commit,
                                        numberLineCustomLabelFamily=self.commit),
                                    _semantic_member_wrappers={})

    def make(self, *values):
        self.created.append(values)
        option = Owned()
        self.options.append(option)
        return option

    def commit(self, *values):
        self.calls.append(values)
        return Owned()

    def invoke(self, values=None, attach=True, excluding=None, **config):
        return labels.number_mobjects(self.axis, values, attach=attach, config=config,
                                      excluding=excluding)

    def axes(self):
        self.axis.add_labels = lambda values, **config: labels.custom_labels(
            self.axis, values, config=config
        )
        self.axis.add_numbers = lambda values=None, excluding=None, **config: setattr(
            self.axis, "numbers", labels.number_mobjects(
                self.axis, values, attach=True, config=config, excluding=excluding,
            )
        )
        self.axis.numbers_to_exclude = (0.0,)
        y_axis = SimpleNamespace(
            _semantic_member_wrappers={},
            numbers_to_exclude=(0.0,),
            _semantic_family_handle=SimpleNamespace(
                numberLabelFamily=self.commit,
                numberLineDecimalNumberLabelFamily=self.commit,
                numberLineCustomLabelFamily=self.commit,
            ),
        )
        y_axis.add_labels = lambda values, **config: labels.custom_labels(
            y_axis, values, config=config
        )
        y_axis.add_numbers = lambda values=None, excluding=None, **config: setattr(
            y_axis, "numbers", labels.number_mobjects(
                y_axis, values, attach=True, config=config, excluding=excluding,
            )
        )
        return SimpleNamespace(x_axis=self.axis,
            y_axis=y_axis,
            _semantic_family_handle=SimpleNamespace(
                coordinateLabelFamilies=self.commit_axes,
                axesDecimalCoordinateLabelFamilies=self.commit_axes,
            ))

    def commit_axes(self, *values):
        self.calls.append(values)
        return Owned(), Owned()

    def test_order_duplicates_and_one_pass_values_are_not_recomputed(self):
        result = self.invoke(iter((2, -1, 2)), font="Fixture Sans",
                             font_size='22', decimal_places=2)
        self.assertEqual(self.calls[0][0:2], ([2., -1., 2.], False))
        self.assertEqual(self.created, [('Fixture Sans', 22., 2, 0.12)])
        self.assertIs(self.axis.numbers, result)
        self.assertIs(self.axis._semantic_member_wrappers[id(result)], result)
        self.assertEqual(self.options[0].freed, 0)  # moved, not manually freed
        self.assertEqual(self.options[0].excluded, [])

    def test_number_line_defaults_to_retained_decimal_number_labels(self):
        family = Owned()
        bridge = Mock(return_value=family)
        wrap = patch.object(labels, "_decimal_family", return_value=family).start()
        patch.object(labels, "_line_decimal_labels", bridge).start()
        result = self.invoke((1, -1), excluding=(1,))
        self.assertIs(result, family)
        bridge.assert_called_once_with(
            self.axis._semantic_family_handle,
            [1.0, -1.0], False, self.options[0], True,
        )
        self.assertEqual(self.options[0].excluded, [1.0])
        self.assertIs(self.axis.numbers, family)
        wrap.assert_called_once_with(family, labels._base.WHITE)

    def test_number_line_numeric_labels_require_the_prepared_tex_backend(self):
        with patch.object(labels, "_line_decimal_labels", None):
            with self.assertRaisesRegex(RuntimeError, "prepare_latex"):
                self.invoke((1,))
        self.assertEqual(self.options[0].freed, 1)
        self.assertEqual(self.calls, [])

    def test_exclusion_values_are_forwarded_once_through_the_owned_option(self):
        self.invoke(iter((0, 1, 2)), font="Fixture Sans", excluding=iter((1,)))
        self.assertEqual(self.options[0].excluded, [1.0])

    def test_automatic_tick_selection_keeps_the_axis_tip_and_origin_policy(self):
        self.axis.include_tip = True
        self.axis.exclude_origin_tick = True
        self.invoke(font="Fixture Sans")
        self.assertEqual(self.options[0].tick_range_flags, (True, True))

    def test_decimal_number_formatting_options_reach_the_shared_rust_adapter(self):
        self.invoke(
            (1234,), font="Fixture Sans", include_sign=True, group_with_commas=False,
            show_ellipsis=True, unit=r"\text{m}",
        )
        options = self.options[0]
        self.assertTrue(options.include_sign)
        self.assertFalse(options.group_with_commas)
        self.assertTrue(options.show_ellipsis)
        self.assertEqual(options.unit, r"\text{m}")
        self.assertIsNone(options.log_base)

    def test_log_base_is_forwarded_to_the_shared_rust_adapter(self):
        options, _, _ = labels._options({"font": "Fixture Sans"}, log_base=10)
        self.assertEqual(self.options[0].log_base, 10.0)

    def test_custom_labels_preserve_mapping_order_and_bind_the_attached_alias(self):
        family = Owned()
        self.wrapped.side_effect = lambda handle, size, color: family
        self.axis._semantic_family_handle.numberLineCustomLabelFamily = Mock(
            return_value=Owned()
        )
        result = labels.custom_labels(
            self.axis,
            {2: "Tuesday", 0: 1},
            config={"font_size": 24, "direction": (0, 1), "buff": 0.2},
        )
        self.assertIs(result, family)
        call = self.axis._semantic_family_handle.numberLineCustomLabelFamily.call_args
        self.assertEqual(call.args[0:2], ([2.0, 0.0], ["Tuesday", "1"]))
        self.assertIs(self.axis.labels, family)
        self.assertIs(self.axis._semantic_member_wrappers[id(family)], family)

    def test_log_axes_defaults_select_exponents_and_explicit_values_keep_numeric_labels(self):
        axes = self.axes()
        axes.x_axis.scaling = plotting.LogBase(base=2)
        axes.x_axis.add_numbers = Mock(side_effect=lambda *args, **kwargs: setattr(
            axes.x_axis, "numbers", "explicit-labels"))
        axes.y_axis.add_numbers = Mock(side_effect=lambda *args, **kwargs: setattr(
            axes.y_axis, "numbers", "linear-labels"))
        with patch.object(labels, "logarithmic_labels", return_value="log-labels") as custom:
            self.assertIs(labels.add_coordinates(axes, config={}), axes)
            custom.assert_called_once_with(axes.x_axis)
            axes.x_axis.add_numbers.assert_not_called()
            self.assertEqual(axes.coordinate_labels, ("log-labels", "linear-labels"))
        with patch.object(labels, "logarithmic_labels") as custom:
            self.assertIs(labels.add_coordinates(axes, (0.5, 4.0), config={}), axes)
            custom.assert_not_called()
            self.assertEqual(axes.x_axis.add_numbers.call_args.args, ((0.5, 4.0),))
            self.assertEqual(axes.coordinate_labels, ("explicit-labels",))

    def test_custom_labels_reject_unsupported_objects_before_allocating(self):
        with self.assertRaises(NotImplementedError):
            labels.custom_labels(self.axis, {1: object()}, config={})
        self.assertEqual(self.options, [])
        self.assertEqual(self.calls, [])

    def test_none_and_empty_are_distinct_and_detached_does_not_attach(self):
        self.invoke(None, attach=False, font="Fixture Sans")
        self.invoke((), attach=False, font="Fixture Sans")
        self.assertEqual([(call[0], call[1], call[3]) for call in self.calls],
                         [([], True, False), ([], False, False)])
        self.assertFalse(hasattr(self.axis, 'numbers'))
        self.assertEqual(self.axis._semantic_member_wrappers, {})

    def test_unknown_options_and_host_coercion_fail_before_allocation(self):
        for config in ({'decimal_places': True}, {'decimal_places': 1.5}, {'label_constructor': object}):
            with self.assertRaises(TypeError):
                self.invoke((1,), font="Fixture Sans", **config)
        with self.assertRaises(ValueError):
            self.invoke(('bad',))
        self.assertEqual(self.options, [])
        self.assertEqual(self.calls, [])

    def test_setter_failure_releases_only_unconsumed_option(self):
        error = ValueError('style rejected')
        def reject(*args):
            raise error
        def make(*args):
            option = self.make(*args)
            option.setColor = reject
            return option
        labels._plot._coordinate_options.numberLabels = make
        with self.assertRaises(ValueError) as caught:
            self.invoke((1,))
        self.assertIs(caught.exception, error)
        self.assertEqual(self.options[0].freed, 1)
        self.assertEqual(self.calls, [])

        def reject_exclusions(*args):
            raise error
        def make_with_exclusion_failure(*args):
            option = self.make(*args)
            option.setExcludedNumbers = reject_exclusions
            return option
        labels._plot._coordinate_options.numberLabels = make_with_exclusion_failure
        with self.assertRaises(ValueError) as caught:
            self.invoke((1,), excluding=(0,))
        self.assertIs(caught.exception, error)
        self.assertEqual(self.options[1].freed, 1)
        self.assertEqual(self.calls, [])

    def test_consuming_rust_failure_preserves_alias_without_double_free(self):
        previous = object()
        self.axis.numbers = previous
        error = ValueError('transaction rejected')
        def reject(*args):
            raise error
        self.axis._semantic_family_handle.numberLabelFamily = reject
        with self.assertRaises(ValueError) as caught:
            self.invoke((1,), font="Fixture Sans")
        self.assertIs(caught.exception, error)
        self.assertIs(self.axis.numbers, previous)
        self.assertEqual(self.options[0].freed, 0)
        self.wrapped.assert_not_called()

    def test_second_axis_options_failure_releases_first_and_never_commits(self):
        with self.assertRaises(TypeError):
            labels.add_coordinates(self.axes(), None, None, x_config=None,
                                   y_config={'font': ''},
                                   config={'font': 'Fixture Sans'})
        self.assertEqual(len(self.options), 1)
        self.assertEqual(self.options[0].freed, 1)
        self.assertEqual(self.calls, [])
        self.assertFalse(hasattr(self.axis, 'numbers'))

    def test_axes_use_one_commit_and_axis_specific_option_precedence(self):
        axes = self.axes()
        result = labels.add_coordinates(axes, iter((2, 1)), (),
            x_config={'direction': labels._base.UP}, y_config=None,
            config={'direction': labels._base.RIGHT, 'font_size': 20,
                    'font': 'Fixture Sans'})
        self.assertIs(result, axes)
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(self.calls[0][:4], ([2., 1.], False, [], False))
        self.assertEqual(self.options[0].direction, (0, 1))
        self.assertEqual(self.options[1].direction, (1, 0))
        self.assertEqual([o.freed for o in self.options], [0, 0])
        self.assertEqual(axes.coordinate_labels,
                         (axes.x_axis.numbers, axes.y_axis.numbers))

    def test_axes_default_numeric_coordinates_use_one_decimal_number_commit(self):
        axes = self.axes()
        x_family, y_family = Owned(), Owned()
        bridge = Mock(return_value=(x_family, y_family))
        patch.object(labels, "_axes_decimal_labels", bridge).start()
        patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle).start()
        result = labels.add_coordinates(axes, None, (1, -1), config={})
        self.assertIs(result, axes)
        bridge.assert_called_once()
        self.assertEqual(bridge.call_args.args[:5], (
            axes._semantic_family_handle, [], True, [1.0, -1.0], False,
        ))
        self.assertEqual([options.exclude for options in self.options], [False, False])
        self.assertEqual([options.excluded for options in self.options], [[0.0], [0.0]])
        self.assertEqual(axes.coordinate_labels,
                         (axes.x_axis.numbers, axes.y_axis.numbers))
        self.assertIs(axes.x_axis.numbers, x_family)
        self.assertIs(axes.y_axis.numbers, y_family)

    def test_atomic_axis_labels_forward_tip_and_origin_policy_for_both_profiles(self):
        for config in ({}, {"font": "Fixture Sans"}):
            axes = self.axes()
            axes.x_axis.include_tip = True
            axes.x_axis.exclude_origin_tick = True
            axes.y_axis.include_tip = False
            axes.y_axis.exclude_origin_tick = False
            with self.subTest(config=config), patch.object(
                labels, "_axes_decimal_labels", return_value=(Owned(), Owned())
            ), patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle):
                labels.add_coordinates(axes, config=config)
            self.assertEqual([option.tick_range_flags for option in self.options[-2:]],
                             [(True, True), (False, False)])

    def test_axes_constructor_decimal_formatting_is_preserved_per_axis(self):
        axes = self.axes()
        axes.x_axis.decimal_number_config = {
            "num_decimal_places": 2,
            "include_sign": True,
            "group_with_commas": False,
            "unit": "m",
        }
        axes.y_axis.decimal_number_config = {
            "num_decimal_places": 1,
            "show_ellipsis": True,
            "unit": "s",
        }
        families = (Owned(), Owned())
        bridge = Mock(return_value=families)
        patch.object(labels, "_axes_decimal_labels", bridge).start()
        patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle).start()

        labels.add_coordinates(axes, (1234,), (2,), config={})

        self.assertEqual(self.created, [
            ("DejaVu Sans Mono", 18.0, 2, 0.12),
            ("DejaVu Sans Mono", 18.0, 1, 0.12),
        ])
        self.assertTrue(self.options[0].include_sign)
        self.assertFalse(self.options[0].group_with_commas)
        self.assertFalse(self.options[0].show_ellipsis)
        self.assertEqual(self.options[0].unit, "m")
        self.assertFalse(self.options[1].include_sign)
        self.assertTrue(self.options[1].group_with_commas)
        self.assertTrue(self.options[1].show_ellipsis)
        self.assertEqual(self.options[1].unit, "s")

        labels.add_coordinates(
            axes, (1234,), (2,), config={"include_sign": False},
            x_config={"include_sign": True, "unit": "km"},
        )
        self.assertTrue(self.options[2].include_sign)
        self.assertEqual(self.options[2].unit, "km")
        self.assertFalse(self.options[3].include_sign)
        self.assertEqual(self.options[3].unit, "s")

    def test_axes_exact_exclusions_follow_axis_config_and_global_override(self):
        axes = self.axes()
        axes.x_axis.numbers_to_exclude = (0.0, 2.0)
        axes.y_axis.numbers_to_exclude = (-1.0,)
        families = (Owned(), Owned())
        patch.object(labels, "_axes_decimal_labels", Mock(return_value=families)).start()
        patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle).start()

        labels.add_coordinates(axes, (0, 1, 2), (-1, 0, 1), config={})
        self.assertEqual([option.excluded for option in self.options],
                         [[0.0, 2.0], [-1.0]])

    def test_axes_explicit_excluding_overrides_both_axis_configs(self):
        axes = self.axes()
        families = (Owned(), Owned())
        patch.object(labels, "_axes_decimal_labels", Mock(return_value=families)).start()
        patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle).start()
        labels.add_coordinates(
            axes, (0, 1, 2), (-1, 0, 1), config={"excluding": iter((2,))},
        )
        self.assertEqual([option.excluded for option in self.options], [[2.0], [2.0]])

    def test_axes_explicit_include_zero_overrides_constructor_exclusion(self):
        axes = self.axes()
        families = (Owned(), Owned())
        patch.object(labels, "_axes_decimal_labels", Mock(return_value=families)).start()
        patch.object(labels, "_decimal_family", side_effect=lambda handle, color: handle).start()
        labels.add_coordinates(axes, (0, 1), (0, 1), config={"exclude_zero": False})
        self.assertEqual([option.excluded for option in self.options], [[], []])
        self.assertEqual([option.exclude for option in self.options], [False, False])

    def test_single_coordinate_argument_labels_only_the_first_axis(self):
        axes = self.axes()
        result = labels.add_coordinates(axes, iter((2, 1)), config={'font': 'Fixture Sans'},
                                        x_config=None, y_config=None)
        self.assertIs(result, axes)
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(self.calls[0][0:2], ([2.0, 1.0], False))
        self.assertEqual(axes.coordinate_labels, (axes.x_axis.numbers,))
        self.assertFalse(hasattr(axes.y_axis, 'numbers'))

    def test_custom_coordinate_mapping_uses_native_labels_and_keeps_omitted_axes_empty(self):
        axes = self.axes()
        result = labels.add_coordinates(axes, {1: "Monday"}, config={"font": "Fixture Sans"},
                                        x_config=None, y_config=None)
        self.assertIs(result, axes)
        self.assertEqual(self.calls[0][0:2], ([1.0], ["Monday"]))
        self.assertIs(axes.coordinate_labels[0], axes.x_axis.labels)
        self.assertEqual(len(axes.coordinate_labels), 1)
        self.assertFalse(hasattr(axes.y_axis, 'numbers'))

    def test_mapping_requires_an_explicit_native_font_before_publication(self):
        axes = self.axes()
        with self.assertRaisesRegex(NotImplementedError, "default TeX mappings"):
            labels.add_coordinates(axes, (1,), {1: "x^2"}, config={})
        self.assertEqual(self.calls, [])
        self.assertEqual(self.options, [])

    def test_custom_mapping_filters_decimal_options_and_preserves_font_on_both_coordinate_types(self):
        for add in (labels.add_coordinates, labels.add_number_plane_coordinates):
            owner = self.axes()
            owner.x_axis.decimal_number_config = {
                "num_decimal_places": 2, "include_sign": True,
                "group_with_commas": False, "unit": "m",
            }
            calls = []
            def custom(axis, values, *, config):
                calls.append((axis, values, config))
                axis.labels = object()
                return axis.labels
            with self.subTest(add=add), patch.object(labels, "custom_labels", side_effect=custom):
                result = add(owner, {1: "Monday"}, config={
                    "font": "Fixture Sans", "num_decimal_places": 0,
                    "include_sign": False, "direction": labels._base.RIGHT,
                    "buff": 0.4, "font_size": 24,
                })
            self.assertIs(result, owner)
            self.assertEqual(calls, [(owner.x_axis, {1: "Monday"}, {
                "font": "Fixture Sans", "direction": labels._base.RIGHT,
                "buff": 0.4, "font_size": 24,
            })])
            self.assertFalse(hasattr(owner.y_axis, "labels"))

    def test_number_plane_defaults_to_decimal_glyph_families_and_one_decimal(self):
        x_axis = SimpleNamespace(
            _semantic_member_wrappers={},
            decimal_number_config={"include_sign": True, "unit": "m"},
        )
        y_axis = SimpleNamespace(
            _semantic_member_wrappers={},
            decimal_number_config={"show_ellipsis": True, "unit": "s"},
        )
        class FamilyHandle:
            def decimalNumberLabelMembers(self):
                return ()
        class PlaneHandle:
            def numberPlaneCoordinateLabelFamilies(self, *args):
                raise AssertionError("native Text route should be opt-in")
        plane_handle = PlaneHandle()
        plane = SimpleNamespace(x_axis=x_axis, y_axis=y_axis,
                                _semantic_family_handle=plane_handle,
                                _coordinate_decimal_places=(2, 3))
        calls = []
        def decimal_bridge(*args):
            calls.append(args)
            return FamilyHandle(), FamilyHandle()
        patch.object(labels, "_plane_decimal_labels", decimal_bridge).start()
        patch.object(labels, "_decimal_members", side_effect=lambda family: family.decimalNumberLabelMembers()).start()
        patch.object(labels._plot, "_family", side_effect=lambda wrapper, handle, members: (handle, members)).start()
        result = labels.add_number_plane_coordinates(
            plane, (1.0,), (-1.0,), x_config=None, y_config=None,
            config={"font_size": 24, "buff": 0.1},
        )
        self.assertIs(result, plane)
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][1:5], ([1.0], False, [-1.0], False))
        self.assertEqual([option.exclude for option in self.options], [False, False])
        self.assertEqual([option.excluded for option in self.options], [[0.0], [0.0]])
        self.assertEqual([entry[2] for entry in self.created], [2, 3])
        self.assertTrue(self.options[0].include_sign)
        self.assertFalse(self.options[1].include_sign)
        self.assertTrue(self.options[1].show_ellipsis)
        self.assertEqual([option.unit for option in self.options], ["m", "s"])
        self.assertEqual(len(x_axis.numbers), 2)
        self.assertEqual(len(y_axis.numbers), 2)

        self.created.clear()
        labels.add_number_plane_coordinates(
            plane, (1.0,), (-1.0,), x_config=None, y_config=None,
            config={"num_decimal_places": 0},
        )
        self.assertEqual([entry[2] for entry in self.created], [0, 0])

    def test_number_plane_single_axis_defaults_keep_decimal_number_profile(self):
        x_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        y_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        plane = SimpleNamespace(
            x_axis=x_axis, y_axis=y_axis,
            _semantic_family_handle=object(),
            _coordinate_decimal_places=(2, 3),
        )
        handle = Owned()
        family = object()
        bridge = patch.object(labels, "_line_decimal_labels", return_value=handle).start()
        patch.object(labels, "_decimal_members", object()).start()
        patch.object(labels, "_decimal_family", return_value=family).start()
        result = labels.add_number_plane_coordinates(
            plane, (1.0,), x_config=None, y_config=None,
            config={"font_size": 24, "buff": 0.1},
        )
        self.assertIs(result, plane)
        self.assertIs(x_axis.numbers, family)
        self.assertFalse(hasattr(y_axis, "numbers"))
        self.assertEqual(plane.coordinate_labels, (family,))
        self.assertEqual(bridge.call_args.args[:3], (x_axis._semantic_family_handle, [1.0], False))
        self.assertEqual(self.created[0][2], 2)

    def test_number_plane_can_mix_one_native_font_axis_with_decimal_labels(self):
        x_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        y_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        native_family, decimal_family = object(), object()
        x_axis.add_numbers = Mock(side_effect=lambda values, **kwargs:
                                  setattr(x_axis, "numbers", native_family))
        plane = SimpleNamespace(
            x_axis=x_axis, y_axis=y_axis,
            _semantic_family_handle=object(),
            _coordinate_decimal_places=(0, 1),
        )
        patch.object(labels, "_line_decimal_labels", return_value=Owned()).start()
        patch.object(labels, "_decimal_members", object()).start()
        patch.object(labels, "_decimal_family", return_value=decimal_family).start()
        labels.add_number_plane_coordinates(
            plane, None, (-1.0, 1.0),
            x_config={"font": "Fixture Sans"}, y_config=None,
            config={"font_size": 24},
        )
        x_axis.add_numbers.assert_called_once_with(
            None, excluding=(0.0,), font_size=24, font="Fixture Sans",
            direction=labels._base.DR, decimal_places=0,
        )
        self.assertIs(y_axis.numbers, decimal_family)
        self.assertEqual(plane.coordinate_labels, (native_family, decimal_family))

    def test_number_plane_requires_prepared_latex_without_native_font_opt_in(self):
        patch.object(labels, "_plane_decimal_labels", None).start()
        plane = SimpleNamespace(x_axis=object(), y_axis=object(),
                                _semantic_family_handle=object())
        with self.assertRaisesRegex(RuntimeError, "prepare_latex"):
            labels.add_number_plane_coordinates(
                plane, None, None, x_config=None, y_config=None, config={}
            )
        self.assertEqual(self.options, [])

    def test_nonempty_decimal_label_family_uses_numeric_facades_without_restyling(self):
        from _manim_numbers import DecimalNumber

        family = Owned()
        object_handle = SimpleNamespace()
        numeric = SimpleNamespace(mobject=lambda: object_handle, text=lambda: "-1.0",
                                  fontSize=lambda context: 18, value=lambda context: -1)
        bridge = patch.object(labels, "_decimal_members", return_value=(numeric,)).start()
        wrap = patch.object(labels._plot, "_family",
                            side_effect=lambda wrapper, handle, members: members).start()
        members = labels._decimal_family(family, labels._base.RED)
        bridge.assert_called_once_with(family)
        wrap.assert_called_once()
        self.assertEqual(len(members), 1)
        number = members[0]
        self.assertIsInstance(number, DecimalNumber)
        self.assertIs(number._semantic_handle, object_handle)
        self.assertEqual(number.source, "-1.0")
        self.assertEqual(number.font_size, 18)
        self.assertEqual(number.get_value(), -1)

    def test_number_plane_global_font_keeps_native_text_path(self):
        x_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        y_axis = SimpleNamespace(_semantic_member_wrappers={}, _semantic_family_handle=object())
        owner = SimpleNamespace(
            x_axis=x_axis, y_axis=y_axis,
            _semantic_family_handle=SimpleNamespace(numberPlaneCoordinateLabelFamilies=self.commit_axes),
        )
        patch.object(labels, "_plane_decimal_labels", None).start()
        labels.add_number_plane_coordinates(
            owner, None, None, x_config=None, y_config=None,
            config={"font": "Fixture Sans"},
        )
        self.assertEqual(len(self.calls), 1)
        self.assertEqual([entry[0] for entry in self.created], ["Fixture Sans", "Fixture Sans"])

    def test_live_and_callback_guards_precede_allocation(self):
        labels._shared._live_constructor_context.return_value = object()
        with self.assertRaises(NotImplementedError):
            self.invoke((1,))
        labels._shared._live_constructor_context.return_value = None
        labels._plot._outside_callback.side_effect = NotImplementedError('callback')
        with self.assertRaises(NotImplementedError):
            self.invoke((1,))
        self.assertEqual(self.options, [])


if __name__ == '__main__':
    unittest.main()
