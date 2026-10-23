"""Host call order only; native and browser tests own semantic mutation proof."""
import unittest

import noon as module
promote = module.Mobject.push_self_into_submobjects


class Copy:
    def __init__(self, events, fail):
        self.events, self.fail = events, fail

    @property
    def submobjects(self):
        raise AssertionError('promotion must assign without querying the copied children')

    @submobjects.setter
    def submobjects(self, value):
        self.events.append(('children', value))
        if self.fail == 'children':
            raise LookupError('children refused')


class Source:
    def __init__(self, fail=None):
        self.events, self.fail = [], fail
        self.clone = Copy(self.events, fail)

    def copy(self):
        self.events.append('copy')
        if self.fail == 'copy':
            raise LookupError('copy refused')
        return self.clone

    def reset_points(self):
        self.events.append('reset')
        if self.fail == 'reset':
            raise LookupError('reset refused')
        return object()

    def add(self, value):
        assert value is self.clone
        self.events.append('add')
        if self.fail == 'add':
            raise LookupError('add refused')
        return object()


class PointPromotionProtocolTests(unittest.TestCase):
    def test_custom_descriptors_are_read_only_at_their_public_call_position(self):
        events = []
        clone = Copy(events, None)
        class Custom(module.Mobject):
            def __init__(self):
                pass

            @property
            def copy(self):
                events.append('copy lookup')
                def invoke():
                    events.append('copy')
                    return clone
                return invoke

            @property
            def reset_points(self):
                events.append('reset lookup')
                return lambda: events.append('reset')

            @property
            def add(self):
                events.append('add lookup')
                return lambda value: events.append(('add', value is clone))
        source = Custom()
        self.assertIs(promote(source), source)
        self.assertEqual(events, ['copy lookup', 'copy', ('children', []),
                                  'reset lookup', 'reset', 'add lookup', ('add', True)])

    def test_success_observes_copy_setter_reset_add_and_returns_original(self):
        source = Source()
        self.assertIs(promote(source), source)
        self.assertEqual(source.events, ['copy', ('children', []), 'reset', 'add'])

    def test_failures_preserve_the_exact_completed_public_call_prefix(self):
        sequence = ['copy', ('children', []), 'reset', 'add']
        for index, phase in enumerate(['copy', 'children', 'reset', 'add']):
            with self.subTest(phase=phase):
                source = Source(phase)
                with self.assertRaisesRegex(LookupError, phase + ' refused'):
                    promote(source)
                self.assertEqual(source.events, sequence[:index + 1])


if __name__ == '__main__':
    unittest.main()
