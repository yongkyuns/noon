"""The SampleSpace facade delegates semantic work and retains handle identity."""
from __future__ import annotations

import unittest
from unittest.mock import patch

import noon
import _manim_compat as compat
import _manim_sample_space as sample_space


class _MobjectHandle:
    def __init__(self, slot: int, generation: int = 1) -> None:
        self.semanticSlot = slot
        self.semanticGeneration = generation


class _FamilyHandle:
    def __init__(self, slot: int, members=()) -> None:
        self.semanticSlot = slot
        self.semanticGeneration = 1
        self._members = list(members)

    @property
    def memberCount(self):
        return len(self._members)

    def memberKeys(self):
        return [f"{member.semanticSlot}:{member.semanticGeneration}"
                for member in self._members]

    def memberMobject(self, index):
        return self._members[index]


class _Options:
    def __init__(self, width, height):
        self.width = width
        self.height = height
        self.values = {}

    def setFillColor(self, *rgba):
        self.values["fill_color"] = rgba

    def setFillOpacity(self, value):
        self.values["fill_opacity"] = value

    def setStrokeColor(self, *rgb):
        self.values["stroke_color"] = rgb

    def setStrokeWidth(self, value):
        self.values["stroke_width"] = value


class _SampleSpaceHandle:
    def __init__(self):
        self._rectangle = _MobjectHandle(1)
        self._family = _FamilyHandle(100, [self._rectangle])
        self._horizontal = None
        self._vertical = None
        self.next_slot = 200
        self.calls = []

    def rectangle(self):
        return self._rectangle

    def family(self):
        return self._family

    def completePList(self, values):
        self.calls.append(("complete", tuple(values)))
        values = list(values)
        remainder = 1.0 - sum(values)
        if abs(remainder) > 0.0001:
            values.append(remainder)
        return values

    def _new_parts(self, probabilities, colors):
        self.calls.append(("parts", tuple(probabilities), tuple(colors)))
        count = len(self.completePList(probabilities))
        family = _FamilyHandle(
            self.next_slot,
            [_MobjectHandle(self.next_slot + i + 1) for i in range(count)],
        )
        self.next_slot += count + 1
        return family

    def getHorizontalDivision(self, probabilities, colors):
        return self._new_parts(probabilities, colors)

    def getVerticalDivision(self, probabilities, colors):
        return self._new_parts(probabilities, colors)

    def divideHorizontally(self, probabilities, colors):
        self._horizontal = self._new_parts(probabilities, colors)
        self._family._members.append(self._horizontal)
        return self._horizontal

    def divideVertically(self, probabilities, colors):
        self._vertical = self._new_parts(probabilities, colors)
        self._family._members.append(self._vertical)
        return self._vertical

    def horizontalParts(self):
        return self._horizontal

    def verticalParts(self):
        return self._vertical


class _AnnotationMobject(noon.Mobject):
    def __init__(self, source=""):
        self.source = source
        self.scaled = None
        self.placement = None

    def scale(self, factor):
        self.scaled = factor
        return self

    def next_to(self, target, direction, buff):
        self.placement = (target, direction, buff)
        return self


class _AnnotationGroup:
    def __init__(self, *members):
        self.submobjects = list(members)

    def copy(self):
        return _AnnotationGroup(*self.submobjects)


class ManimSampleSpaceBridgeTests(unittest.TestCase):
    def setUp(self):
        self.handles = []
        self.created_options = []

        def create(options):
            self.created_options.append(options)
            handle = _SampleSpaceHandle()
            self.handles.append(handle)
            return handle

        self.patches = [
            patch.object(sample_space, "_sample_space_options", type(
                "OptionsFactory", (), {"new": staticmethod(_Options)}
            )),
            patch.object(sample_space, "_create_sample_space", create),
            patch.object(sample_space, "_to_js", lambda values: list(values)),
            patch.object(sample_space._shared, "_live_constructor_context", return_value=None),
            patch.object(sample_space._shared, "_live_mutation_context", return_value=None),
        ]
        for active in self.patches:
            active.start()
            self.addCleanup(active.stop)

    def test_public_export_and_default_construction_use_one_opaque_family(self):
        space = sample_space.SampleSpace()

        self.assertIs(noon.SampleSpace, sample_space.SampleSpace)
        self.assertIsInstance(space, compat.Group)
        self.assertEqual(len(space.submobjects), 1)
        self.assertIsInstance(space.submobjects[0], compat.Rectangle)
        options = self.created_options[0]
        self.assertEqual((options.width, options.height), (3.0, 3.0))
        self.assertEqual(options.values["fill_opacity"], 1.0)
        self.assertEqual(options.values["stroke_width"], 0.5)

    def test_partitions_and_queries_reuse_wrapper_identity_and_keep_nested_members(self):
        space = sample_space.SampleSpace(height=2.0, width=4.0)
        space.divide_horizontally([0.25, 0.5], colors=[noon.GREEN_E, noon.BLUE_E])

        parts = space.horizontal_parts
        self.assertIs(parts, space.submobjects[1])
        self.assertIs(parts, space.horizontal_parts)
        self.assertEqual(len(parts.submobjects), 3)
        self.assertTrue(all(isinstance(member, compat.Rectangle) for member in parts.submobjects))
        self.assertEqual(len(self.handles[0].calls[-1][1]), 2)

        detached = space.get_vertical_division([0.4], colors=[noon.YELLOW])
        self.assertIsInstance(detached, compat.Group)
        self.assertEqual(len(detached.submobjects), 2)
        self.assertEqual(len(space.submobjects), 2)
        self.assertIsNone(space.vertical_parts)

    def test_probability_query_runs_through_the_typed_handle(self):
        space = sample_space.SampleSpace()

        self.assertEqual(space.complete_p_list([0.25, 0.5]), [0.25, 0.5, 0.25])
        self.assertEqual(space._sample_space_handle.calls, [("complete", (0.25, 0.5))])

    def test_subdivision_brace_label_helpers_preserve_rust_partition_wrappers(self):
        space = sample_space.SampleSpace(default_label_scale_val=0.75)
        space.divide_horizontally([0.25, 0.5])
        parts = space.horizontal_parts
        existing_label = _AnnotationMobject("existing")
        brace_calls = []

        def make_brace(part, direction, *, buff):
            brace = _AnnotationMobject("brace")
            brace_calls.append((part, direction, buff, brace))
            return brace

        with patch.object(noon, "Brace", side_effect=make_brace, create=True), \
             patch.object(noon, "MathTex", side_effect=lambda text: _AnnotationMobject(text), create=True), \
             patch.object(compat, "VGroup", _AnnotationGroup):
            annotations = space.get_side_braces_and_labels(["A", existing_label, "C"])

        self.assertEqual(len(brace_calls), 3)
        self.assertEqual(len(parts.braces.submobjects), 3)
        self.assertEqual([label.source for label in parts.labels.submobjects], ["A", "existing", "C"])
        self.assertEqual(parts.labels.submobjects[1], existing_label)
        self.assertEqual(parts.labels.submobjects[0].scaled, 0.75)
        self.assertEqual(parts.labels.submobjects[2].scaled, 0.75)
        self.assertEqual(brace_calls[0][1], noon.LEFT)
        self.assertEqual(parts.label_kwargs["direction"], noon.LEFT)
        self.assertEqual(parts.label_kwargs["buff"], noon.SMALL_BUFF)
        self.assertIsNot(parts.label_kwargs["labels"], parts.labels)
        self.assertEqual(len(annotations.submobjects), 2)

        with patch.object(noon, "Brace", side_effect=make_brace, create=True), \
             patch.object(noon, "MathTex", side_effect=lambda text: _AnnotationMobject(text), create=True), \
             patch.object(compat, "VGroup", _AnnotationGroup):
            space.divide_vertically([0.5])
            top = space.get_top_braces_and_labels(["left", "right"])
            bottom = space.get_bottom_braces_and_labels(["left", "right"])
        self.assertEqual(top.submobjects[1].submobjects[0].placement[1], noon.UP)
        self.assertEqual(bottom.submobjects[1].submobjects[0].placement[1], noon.DOWN)
        attached = []
        with patch.object(space, "add", side_effect=lambda *members: attached.extend(members) or space):
            self.assertIs(space.add_braces_and_labels(), space)
        self.assertEqual(len(attached), 4)

    def test_subdivision_helper_rejects_unimplemented_brace_options(self):
        space = sample_space.SampleSpace()
        space.divide_horizontally([0.5])
        with self.assertRaises(NotImplementedError):
            space.get_side_braces_and_labels(["A", "B"], min_num_quads=2)


if __name__ == "__main__":
    unittest.main()
