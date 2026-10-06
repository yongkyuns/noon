"""Argument/ownership tests only; shared Rust and Pyodide qualify semantics."""
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import _manim_number_labels as labels


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
    def setColor(self, *value):
        self.color = value


class NumericLabelAdapterTests(unittest.TestCase):
    def setUp(self):
        self.options = []
        self.created = []
        self.calls = []
        self.addCleanup(patch.stopall)
        patch.object(labels._plot, '_coordinate_options', SimpleNamespace(numberLabels=self.make)).start()
        patch.object(labels._plot, '_to_js', side_effect=lambda x: x).start()
        patch.object(labels._plot, '_outside_callback').start()
        patch.object(labels._shared, '_live_constructor_context', return_value=None).start()
        patch.object(labels, 'engine_call', side_effect=lambda fn, *args: fn(*args)).start()
        self.wrapped = patch.object(labels, '_family', side_effect=lambda handle, *args: handle).start()
        patch.object(labels._shared, '_family_wrapper_key', side_effect=id).start()
        self.axis = SimpleNamespace(_semantic_family_handle=SimpleNamespace(numberLabelFamily=self.commit),
                                    _semantic_member_wrappers={})

    def make(self, *values):
        self.created.append(values)
        option = Owned()
        self.options.append(option)
        return option

    def commit(self, *values):
        self.calls.append(values)
        return Owned()

    def invoke(self, values=None, attach=True, **config):
        return labels.number_mobjects(self.axis, values, attach=attach, config=config)

    def axes(self):
        return SimpleNamespace(x_axis=self.axis,
            y_axis=SimpleNamespace(_semantic_member_wrappers={}),
            _semantic_family_handle=SimpleNamespace(coordinateLabelFamilies=self.commit_axes))

    def commit_axes(self, *values):
        self.calls.append(values)
        return Owned(), Owned()

    def test_order_duplicates_and_one_pass_values_are_not_recomputed(self):
        result = self.invoke(iter((2, -1, 2)), font_size='22', decimal_places=2)
        self.assertEqual(self.calls[0][0:2], ([2., -1., 2.], False))
        self.assertEqual(self.created, [('DejaVu Sans Mono', 22., 2, 0.12)])
        self.assertIs(self.axis.numbers, result)
        self.assertIs(self.axis._semantic_member_wrappers[id(result)], result)
        self.assertEqual(self.options[0].freed, 0)  # moved, not manually freed

    def test_none_and_empty_are_distinct_and_detached_does_not_attach(self):
        self.invoke(None, attach=False)
        self.invoke((), attach=False)
        self.assertEqual([(call[0], call[1], call[3]) for call in self.calls],
                         [([], True, False), ([], False, False)])
        self.assertFalse(hasattr(self.axis, 'numbers'))
        self.assertEqual(self.axis._semantic_member_wrappers, {})

    def test_unknown_options_and_host_coercion_fail_before_allocation(self):
        for config in ({'decimal_places': True}, {'decimal_places': 1.5}, {'label_constructor': object}):
            with self.assertRaises(TypeError):
                self.invoke((1,), **config)
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

    def test_consuming_rust_failure_preserves_alias_without_double_free(self):
        previous = object()
        self.axis.numbers = previous
        error = ValueError('transaction rejected')
        def reject(*args):
            raise error
        self.axis._semantic_family_handle.numberLabelFamily = reject
        with self.assertRaises(ValueError) as caught:
            self.invoke((1,))
        self.assertIs(caught.exception, error)
        self.assertIs(self.axis.numbers, previous)
        self.assertEqual(self.options[0].freed, 0)
        self.wrapped.assert_not_called()

    def test_second_axis_options_failure_releases_first_and_never_commits(self):
        with self.assertRaises(TypeError):
            labels.add_coordinates(self.axes(), None, None, x_config=None,
                                   y_config={'decimal_places': False}, config={})
        self.assertEqual(len(self.options), 1)
        self.assertEqual(self.options[0].freed, 1)
        self.assertEqual(self.calls, [])
        self.assertFalse(hasattr(self.axis, 'numbers'))

    def test_axes_use_one_commit_and_axis_specific_option_precedence(self):
        axes = self.axes()
        result = labels.add_coordinates(axes, iter((2, 1)), (),
            x_config={'direction': labels._base.UP}, y_config=None,
            config={'direction': labels._base.RIGHT, 'font_size': 20})
        self.assertIs(result, axes)
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(self.calls[0][:4], ([2., 1.], False, [], False))
        self.assertEqual(self.options[0].direction, (0, 1))
        self.assertEqual(self.options[1].direction, (1, 0))
        self.assertEqual([o.freed for o in self.options], [0, 0])

    def test_number_plane_defaults_to_decimal_glyph_families_and_one_decimal(self):
        x_axis = SimpleNamespace(_semantic_member_wrappers={})
        y_axis = SimpleNamespace(_semantic_member_wrappers={})
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
        self.assertEqual([entry[2] for entry in self.created], [2, 3])
        self.assertEqual(len(x_axis.numbers), 2)
        self.assertEqual(len(y_axis.numbers), 2)

        self.created.clear()
        labels.add_number_plane_coordinates(
            plane, (1.0,), (-1.0,), x_config=None, y_config=None,
            config={"num_decimal_places": 0},
        )
        self.assertEqual([entry[2] for entry in self.created], [0, 0])

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

    def test_number_plane_rejects_single_axis_native_font_fallback(self):
        plane = SimpleNamespace(x_axis=object(), y_axis=object(),
                                _semantic_family_handle=object())
        with self.assertRaisesRegex(NotImplementedError, "both NumberPlane axes"):
            labels.add_number_plane_coordinates(
                plane, None, None, x_config={"font": "Fixture Sans"},
                y_config=None, config={},
            )
        self.assertEqual(self.options, [])

    def test_number_plane_global_font_keeps_native_text_path(self):
        x_axis = SimpleNamespace(_semantic_member_wrappers={})
        y_axis = SimpleNamespace(_semantic_member_wrappers={})
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
