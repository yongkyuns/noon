"""Bounds matchers must use the current shared owner, never authored fallback."""
from __future__ import annotations

from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import _manim_shared_geometry as geometry


class ShapeMatcherObservationTests(unittest.TestCase):
    def leaf(self, *, bound=True):
        target = object.__new__(geometry._compat.Rectangle)
        handle = Mock(name="authored_handle")
        geometry._shared._attach_shared_handle(target, handle)
        observation = Mock(name="effective_layout")
        context = SimpleNamespace(queryMobjectLayout=Mock(return_value=observation))
        if bound:
            target._scene = SimpleNamespace(_canonical_authoring_context=context)
            target._object = object()
        return target, handle, observation, context

    def test_detached_target_keeps_authored_handle(self):
        target, handle, _, context = self.leaf(bound=False)
        self.assertIs(geometry._shape_matcher_target(target), handle)
        context.queryMobjectLayout.assert_not_called()

    def test_bound_target_observes_current_owner_once(self):
        target, handle, observation, context = self.leaf()
        self.assertIs(geometry._shape_matcher_target(target), observation)
        context.queryMobjectLayout.assert_called_once_with(handle)

    def test_missing_bound_observation_cannot_fall_back_to_authored(self):
        target, _, _, context = self.leaf()
        context.queryMobjectLayout = None
        with self.assertRaisesRegex(NotImplementedError, "current shared runtime layout"):
            geometry._shape_matcher_target(target)

    def test_rejected_current_observation_is_not_bypassed(self):
        target, _, _, context = self.leaf()
        context.queryMobjectLayout.side_effect = RuntimeError("execution is transferred")
        with self.assertRaisesRegex(RuntimeError, "execution is transferred"):
            geometry._shape_matcher_target(target)

    def test_invalid_handle_cannot_obtain_a_current_observation(self):
        target, _, _, context = self.leaf()
        target._semantic_handle_fresh = False
        with self.assertRaisesRegex(NotImplementedError, "current shared semantic geometry"):
            geometry._shape_matcher_target(target)
        context.queryMobjectLayout.assert_not_called()

    def test_family_uses_existing_current_layout_observation(self):
        target = object.__new__(geometry._compat.VGroup)
        target._semantic_family_handle = object()
        observation = object()
        with patch.object(geometry._shared, "_group_layout_observation", return_value=observation) as query:
            self.assertIs(geometry._shape_matcher_target(target), observation)
            query.assert_called_once_with(target)

    def test_missing_family_handle_is_rejected(self):
        target = object.__new__(geometry._compat.VGroup)
        target._semantic_family_handle = None
        with self.assertRaisesRegex(NotImplementedError, "shared semantic family bounds"):
            geometry._shape_matcher_target(target)

    def test_all_matcher_candidates_use_the_current_observation(self):
        for method, args in (
            ("beginSurroundingRectangle", (0.1, 0.2, 0.0)),
            ("beginBackgroundRectangle", (0.1, 0.2, 0.0, 0.75)),
        ):
            with self.subTest(method=method):
                target, handle, observation, context = self.leaf()
                candidate = object()
                getattr(observation, method).return_value = candidate
                with patch.object(geometry._shared, "_create_geometry_handle", object()):
                    result = geometry._shape_matcher_options(target, method, *args)
                self.assertIs(result, candidate)
                context.queryMobjectLayout.assert_called_once_with(handle)
                getattr(observation, method).assert_called_once_with(*args)
                getattr(handle, method).assert_not_called()

    def test_cross_children_share_one_observation_across_publication(self):
        target, handle, observation, context = self.leaf()
        first, second = Mock(name="first_line"), Mock(name="second_line")
        observation.beginCrossLine.side_effect = [first, second]
        attached = []

        def attach(member, candidate, kind):
            attached.append((member, candidate, kind))
            # Creating the first child may publish semantic state. The second
            # must still derive from the one observation captured for this Cross.
            context.queryMobjectLayout.return_value = Mock(name="later_layout")

        with (
            patch.object(geometry._shared, "_create_geometry_handle", object()),
            patch.object(geometry._shared, "_geometry_options", object()),
            patch.object(geometry._shared, "_attach_geometry_options", side_effect=attach),
            patch.object(geometry._compat.VGroup, "__init__", return_value=None),
        ):
            cross = geometry.Cross(target, scale_factor=1.5)
        self.assertIsInstance(cross, geometry._compat.VGroup)
        self.assertEqual(len(attached), 2)
        self.assertTrue(all(isinstance(item[0], geometry._compat.Line) for item in attached))
        self.assertIs(attached[0][1], first)
        self.assertIs(attached[1][1], second)
        self.assertEqual(observation.beginCrossLine.call_args_list, [unittest.mock.call(0, 1.5), unittest.mock.call(1, 1.5)])
        context.queryMobjectLayout.assert_called_once_with(handle)
        handle.beginCrossLine.assert_not_called()


if __name__ == "__main__":
    unittest.main()
