import dataclasses
import unittest

import _manim_typst as typst


class _Part:
    def __init__(
        self,
        source_start,
        source_end,
        first_cluster,
        cluster_count,
        first_vector,
        vector_count,
        semantic_key,
    ):
        self.sourceStart = source_start
        self.sourceEnd = source_end
        self.firstCluster = first_cluster
        self.clusterCount = cluster_count
        self.firstVector = first_vector
        self.vectorCount = vector_count
        self.semanticKey = semantic_key


class _PartList:
    def __init__(self, parts):
        self._parts = parts
        self.length = len(parts)

    def item(self, index):
        return self._parts[index]


class _Handle:
    def __init__(self, parts):
        self.parts = parts
        self.needles = []

    def textSourcePartsFor(self, needle):
        self.needles.append(needle)
        return _PartList(self.parts)


class ManimTextSourcePartTests(unittest.TestCase):
    def test_python_copies_typed_rust_part_observations_without_rematching_source(self):
        handle = _Handle(
            [
                _Part(0, 2, 0, 1, 0, 0, "noon:text:source:0:2"),
                _Part(9, 11, 7, 1, 3, 2, "noon:text:source:9:11"),
            ]
        )
        label = object.__new__(typst._RetainedTextMobject)
        label._source = "this deliberately does not contain the needle"
        label._semantic_handle = handle

        parts = label.source_parts_for("é")

        self.assertEqual(handle.needles, ["é"])
        self.assertIsInstance(parts, tuple)
        self.assertEqual(
            parts,
            (
                typst.TextSourcePart(0, 2, 0, 1, 0, 0, "noon:text:source:0:2"),
                typst.TextSourcePart(9, 11, 7, 1, 3, 2, "noon:text:source:9:11"),
            ),
        )
        with self.assertRaises(dataclasses.FrozenInstanceError):
            parts[0].source_start = 4

    def test_source_part_query_validates_only_python_argument_shape(self):
        label = object.__new__(typst._RetainedTextMobject)
        label._semantic_handle = _Handle([])

        with self.assertRaises(TypeError):
            label.source_parts_for(123)

    def test_source_part_query_requires_shared_semantic_handle_capability(self):
        label = object.__new__(typst._RetainedTextMobject)
        label._semantic_handle = object()

        with self.assertRaises(NotImplementedError):
            label.source_parts_for("x")


if __name__ == "__main__":
    unittest.main()
