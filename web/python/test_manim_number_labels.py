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
