import unittest

from noon import (
    Color,
    Scene,
    TransformMatchingShapes,
    VectorPath,
)


class TransformMatchingShapesTests(unittest.TestCase):
    def _build_scene(self) -> Scene:
        scene = Scene()
        source_circle_a = scene.circle(1.0, key="source-circle-a")
        source_rectangle = scene.rectangle(2.0, 1.0, key="source-rectangle")
        source_circle_b = scene.circle(0.5, key="source-circle-b")

        target_rectangle = scene.rectangle(
            4.0,
            2.0,
            position=(4.0, 0.0),
            key="target-rectangle",
        )
        target_circle_a = scene.circle(
            2.0,
            position=(1.0, 2.0),
            fill=Color(0.2, 0.6, 0.9),
            key="target-circle-a",
        )
        target_circle_b = scene.circle(
            3.0,
            position=(-3.0, -1.0),
            key="target-circle-b",
        )

        scene.play(
            TransformMatchingShapes(
                [source_circle_a, source_rectangle, source_circle_b],
                [target_rectangle, target_circle_a, target_circle_b],
                key="rearrange",
            ),
            duration=2.0,
            start_time=1.0,
            easing="ease_in_out_cubic",
        )
        return scene

    def test_matches_by_shape_with_stable_duplicate_tie_breaking(self) -> None:
        scene = self._build_scene()
        document = scene.to_document()
        transforms = [
            track for track in document["tracks"] if track["property"] == "transform"
        ]

        self.assertEqual([track["object"] for track in transforms], [0, 1, 2])
        self.assertEqual(
            [track["values"]["object"]["to"]["geometry"] for track in transforms],
            [
                {"circle": {"radius": 2.0}},
                {"rectangle": {"size": {"x": 4.0, "y": 2.0}}},
                {"circle": {"radius": 3.0}},
            ],
        )
        self.assertEqual(
            [track["timing"] for track in transforms],
            [
                {
                    "start_time": 1.0,
                    "duration": 2.0,
                    "easing": "ease_in_out_cubic",
                }
            ]
            * 3,
        )

        identities = scene.identity_document()["tracks"]
        transform_keys = [
            entry["key"]
            for entry, track in zip(identities, document["tracks"])
            if track["property"] == "transform"
        ]
        self.assertEqual(
            transform_keys,
            [
                "rearrange.match:0",
                "rearrange.match:1",
                "rearrange.match:2",
            ],
        )

    def test_identity_is_deterministic_across_equivalent_reruns(self) -> None:
        self.assertEqual(
            self._build_scene().identity_document(),
            self._build_scene().identity_document(),
        )

    def test_rectangle_matching_uses_normalized_aspect_ratio(self) -> None:
        scene = Scene()
        source = scene.rectangle(2.0, 1.0)
        target = scene.rectangle(1.0, 2.0)
        scene.play(TransformMatchingShapes([source], [target]), duration=1.0)

        transform = next(
            track
            for track in scene.to_document()["tracks"]
            if track["property"] == "transform"
        )
        self.assertEqual(
            transform["values"]["object"]["to"]["geometry"],
            {"rectangle": {"size": {"x": 1.0, "y": 2.0}}},
        )

    def test_exact_local_vector_paths_match(self) -> None:
        def triangle() -> VectorPath:
            return (
                VectorPath()
                .move_to((0.0, 1.0))
                .line_to((-1.0, -1.0))
                .line_to((1.0, -1.0))
                .close()
            )

        scene = Scene()
        source = scene.path(triangle(), key="source")
        target = scene.path(
            triangle(),
            position=(3.0, 2.0),
            fill=Color(0.8, 0.3, 0.2),
            key="target",
        )
        scene.play(TransformMatchingShapes([source], [target]), duration=1.0)

        transform = next(
            track
            for track in scene.to_document()["tracks"]
            if track["property"] == "transform"
        )
        self.assertEqual(
            transform["values"]["object"]["from"]["geometry"],
            transform["values"]["object"]["to"]["geometry"],
        )
        self.assertEqual(
            transform["values"]["object"]["to"]["transform"]["translation"],
            {"x": 3.0, "y": 2.0},
        )

    def test_mismatch_is_rejected_transactionally(self) -> None:
        scene = Scene()
        source_circle = scene.circle(1.0)
        source_rectangle = scene.rectangle(2.0, 1.0)
        target_circle_a = scene.circle(2.0)
        target_circle_b = scene.circle(3.0)
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "unmatched shape"):
            scene.play(
                TransformMatchingShapes(
                    [source_circle, source_rectangle],
                    [target_circle_a, target_circle_b],
                ),
                duration=1.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_duplicate_overlap_and_foreign_objects_are_rejected(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        other = scene.circle(3.0)
        foreign = Scene().circle(4.0)

        with self.assertRaisesRegex(ValueError, "unique"):
            scene.play(
                TransformMatchingShapes([source, source], [target, other]),
                duration=1.0,
            )
        with self.assertRaisesRegex(ValueError, "disjoint"):
            scene.play(
                TransformMatchingShapes([source], [source]),
                duration=1.0,
            )
        with self.assertRaisesRegex(ValueError, "belong"):
            scene.play(
                TransformMatchingShapes([source], [foreign]),
                duration=1.0,
            )


if __name__ == "__main__":
    unittest.main()
