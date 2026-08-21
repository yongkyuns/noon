import unittest

from noon import Circle, ReplacementTransform, Scene, Transform, TransformFromCopy


class EvaluatedAuthoringSnapshotTests(unittest.TestCase):
    def test_transform_starts_from_evaluated_narrow_property_state(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        scene.animate_position(
            source,
            (0.0, 0.0),
            (4.0, 2.0),
            duration=4.0,
            start_time=0.0,
        )
        scene.animate_rotation(
            source,
            0.0,
            2.0,
            duration=4.0,
            start_time=0.0,
            easing="ease_in_out_cubic",
        )
        scene.animate_opacity(
            source,
            1.0,
            0.2,
            duration=4.0,
            start_time=0.0,
        )

        scene.play(
            Transform(source, Circle(2.0)),
            duration=1.0,
            start_time=2.0,
        )

        transform = scene.to_document()["tracks"][-1]
        snapshot = transform["values"]["object"]["from"]
        self.assertEqual(snapshot["transform"]["translation"], {"x": 2.0, "y": 1.0})
        self.assertEqual(snapshot["transform"]["rotation"], 1.0)
        self.assertAlmostEqual(snapshot["style"]["opacity"], 0.6)

    def test_latest_started_narrow_track_matches_runtime_precedence(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        scene.animate_position(
            source,
            (0.0, 0.0),
            (10.0, 0.0),
            duration=10.0,
            start_time=0.0,
        )
        scene.animate_position(
            source,
            (100.0, 0.0),
            (200.0, 0.0),
            duration=2.0,
            start_time=2.0,
        )

        scene.play(
            Transform(source, Circle(2.0)),
            duration=1.0,
            start_time=3.0,
        )

        snapshot = scene.to_document()["tracks"][-1]["values"]["object"]["from"]
        self.assertEqual(snapshot["transform"]["translation"], {"x": 150.0, "y": 0.0})

    def test_lifecycle_target_can_use_completed_transform_state(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        scene.play(
            Transform(target, Circle(3.0, position=(4.0, -1.0))),
            duration=1.0,
            start_time=0.0,
        )

        scene.play(
            ReplacementTransform(source, target),
            duration=1.0,
            start_time=2.0,
        )

        transforms = [
            track
            for track in scene.to_document()["tracks"]
            if track["property"] == "transform"
        ]
        replacement = transforms[-1]
        target_snapshot = replacement["values"]["object"]["to"]
        self.assertEqual(target_snapshot["geometry"], {"circle": {"radius": 3.0}})
        self.assertEqual(
            target_snapshot["transform"]["translation"], {"x": 4.0, "y": -1.0}
        )

    def test_copy_can_precede_already_scheduled_future_source_transform(self) -> None:
        scene = Scene()
        source = scene.circle(1.0, position=(-1.0, 0.0))
        target = scene.circle(2.0)
        scene.play(
            Transform(source, Circle(1.5, position=(5.0, 0.0))),
            duration=1.0,
            start_time=5.0,
        )

        scene.play(
            TransformFromCopy(source, target),
            duration=1.0,
            start_time=1.0,
        )

        copy_snapshot = {
            key: value
            for key, value in scene.to_document()["objects"][2].items()
            if key != "id"
        }
        self.assertEqual(
            copy_snapshot["transform"]["translation"], {"x": -1.0, "y": 0.0}
        )

    def test_lifecycle_snapshot_rejects_unrepresentable_reveal_state(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        scene.animate_reveal(source, duration=2.0)
        before = scene.to_document()

        with self.assertRaisesRegex(ValueError, "reveal"):
            scene.play(
                TransformFromCopy(source, target),
                duration=1.0,
                start_time=1.0,
            )

        self.assertEqual(scene.to_document(), before)

    def test_lifecycle_target_rejects_active_generic_transform_at_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        scene.play(Transform(target, Circle(3.0)), duration=4.0)
        before = scene.to_document()

        with self.assertRaisesRegex(ValueError, "active generic Transform"):
            scene.play(
                ReplacementTransform(source, target),
                duration=1.0,
                start_time=1.0,
            )

        self.assertEqual(scene.to_document(), before)

    def test_replacement_rejects_source_narrow_track_before_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0, position=(4.0, 0.0))
        scene.animate_position(
            source,
            (0.0, 0.0),
            (3.0, 0.0),
            duration=1.0,
            start_time=0.0,
        )
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "replacement source.*position"):
            scene.play(
                ReplacementTransform(source, target),
                duration=1.0,
                start_time=2.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_replacement_allows_source_narrow_track_after_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0, position=(4.0, 0.0))
        scene.animate_position(
            source,
            (0.0, 0.0),
            (3.0, 0.0),
            duration=1.0,
            start_time=4.0,
        )

        scene.play(
            ReplacementTransform(source, target),
            duration=1.0,
            start_time=1.0,
        )

        replacement = next(
            track
            for track in scene.to_document()["tracks"]
            if track["property"] == "transform"
        )
        self.assertEqual(replacement["timing"]["start_time"], 1.0)


if __name__ == "__main__":
    unittest.main()
