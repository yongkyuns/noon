"""Public callback protocol only; shared Rust/browser tests own semantic effects."""
import unittest

import noon as module
align = module.Mobject.null_point_align


class Probe:
    def __init__(self, name, points, events):
        self.name, self.points, self.events = name, points, events
        self.on_empty = self.on_push = None

    def has_no_points(self):
        self.events.append(self.name + '.empty')
        return self.on_empty() if self.on_empty else not self.points

    def has_points(self):
        self.events.append(self.name + '.points')
        return self.points

    def push_self_into_submobjects(self):
        self.events.append(self.name + '.push')
        if self.on_push:
            self.on_push()
        self.points = False


class NullPointProtocolTests(unittest.TestCase):
    def test_empty_left_promotes_right_and_then_observes_its_new_predicate(self):
        events = []
        left, right = Probe('left', False, events), Probe('right', True, events)
        self.assertIs(align(left, mobject=right), left)
        self.assertEqual(events, ['left.empty', 'right.points', 'right.push',
                                  'right.empty', 'left.points'])

    def test_pointful_left_short_circuits_the_first_right_predicate(self):
        events = []
        left, right = Probe('left', True, events), Probe('right', False, events)
        self.assertIs(align(left, right), left)
        self.assertEqual(events, ['left.empty', 'right.empty', 'left.points', 'left.push'])

    def test_second_half_reads_mutations_made_by_the_first_public_hook(self):
        events = []
        left, right = Probe('left', False, events), Probe('right', True, events)
        right.on_push = lambda: setattr(left, 'points', True)
        self.assertIs(align(left, right), left)
        self.assertEqual(events, ['left.empty', 'right.points', 'right.push',
                                  'right.empty', 'left.points', 'left.push'])

    def test_predicate_failure_prevents_later_callbacks(self):
        events = []
        left, right = Probe('left', True, events), Probe('right', True, events)
        def reject():
            raise LookupError('empty predicate refused')
        left.on_empty = reject
        with self.assertRaisesRegex(LookupError, 'empty predicate refused'):
            align(left, right)
        self.assertEqual(events, ['left.empty'])

    def test_promotion_failure_prevents_the_second_half(self):
        events = []
        left, right = Probe('left', False, events), Probe('right', True, events)
        def reject():
            raise RuntimeError('promotion refused')
        right.on_push = reject
        with self.assertRaisesRegex(RuntimeError, 'promotion refused'):
            align(left, right)
        self.assertEqual(events, ['left.empty', 'right.points', 'right.push'])


if __name__ == '__main__':
    unittest.main()
