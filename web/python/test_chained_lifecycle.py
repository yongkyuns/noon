import unittest

from noon import ReplacementTransform, Scene, TransformFromCopy


class ChainedLifecycleTests(unittest.TestCase):
    def test_replacement_chain_is_continuous_at_exact_handoff_boundary(self) -> None:
        scene = Scene()
        first = scene.circle(1.0, key="first")
        second = scene.circle(2.0, key="second")
        third = scene.circle(3.0, key="third")

        scene.play(
            ReplacementTransform(first, second, key="first-to-second"),
            duration=1.0,
            start_time=0.0,
        )
        scene.play(
            ReplacementTransform(second, third, key="second-to-third"),
            duration=1.0,
            start_time=1.0,
        )

        second_presence = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == second.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in second_presence],
            [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in second_presence],
            [1.0, 2.0],
        )

    def test_hidden_object_cannot_be_reused_as_source(self) -> None:
        scene = Scene()
        first = scene.circle(1.0)
        second = scene.circle(2.0)
        third = scene.circle(3.0)
        scene.play(ReplacementTransform(first, second), duration=1.0)
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "must be present"):
            scene.play(
                ReplacementTransform(first, third),
                duration=1.0,
                start_time=2.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_visible_lifecycle_object_cannot_be_used_as_target(self) -> None:
        scene = Scene()
        first = scene.circle(1.0)
        second = scene.circle(2.0)
        scene.play(ReplacementTransform(first, second), duration=1.0)
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "target must be absent"):
            scene.play(
                TransformFromCopy(second, second),
                duration=1.0,
                start_time=2.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_visible_previous_target_is_rejected_as_new_distinct_target(self) -> None:
        scene = Scene()
        first = scene.circle(1.0)
        visible_target = scene.circle(2.0)
        source = scene.circle(3.0)
        scene.play(ReplacementTransform(first, visible_target), duration=1.0)
        before_document = scene.to_document()

        with self.assertRaisesRegex(ValueError, "target must be absent"):
            scene.play(
                TransformFromCopy(source, visible_target),
                duration=1.0,
                start_time=2.0,
            )

        self.assertEqual(scene.to_document(), before_document)

    def test_retroactive_lifecycle_authoring_is_rejected_transactionally(self) -> None:
        scene = Scene()
        first = scene.circle(1.0)
        second = scene.circle(2.0)
        third = scene.circle(3.0)
        scene.play(
            ReplacementTransform(first, second),
            duration=1.0,
            start_time=2.0,
        )
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "authored chronologically"):
            scene.play(
                ReplacementTransform(second, third),
                duration=1.0,
                start_time=1.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_copy_handoff_target_can_become_later_replacement_source(self) -> None:
        scene = Scene()
        source = scene.circle(1.0, key="source")
        middle = scene.circle(2.0, key="middle")
        target = scene.circle(3.0, key="target")

        scene.play(
            TransformFromCopy(source, middle, key="copy-to-middle"),
            duration=1.0,
        )
        scene.play(
            ReplacementTransform(middle, target, key="middle-to-target"),
            duration=1.0,
            start_time=1.0,
        )

        middle_presence = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == middle.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in middle_presence],
            [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ],
        )


if __name__ == "__main__":
    unittest.main()
