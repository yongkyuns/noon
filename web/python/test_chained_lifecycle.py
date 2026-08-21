import unittest

from noon import ReplacementTransform, Scene, TransformFromCopy


class ChainedLifecycleTests(unittest.TestCase):
    def test_replacement_chain_is_continuous_at_exact_boundary(self) -> None:
        scene = Scene()
        first = scene.circle(1.0, key="first")
        second = scene.circle(2.0, key="second")
        third = scene.circle(3.0, key="third")

        scene.play(ReplacementTransform(first, second), duration=1.0)
        scene.play(
            ReplacementTransform(second, third),
            duration=1.0,
            start_time=1.0,
        )

        events = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == second.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in events],
            [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in events], [1.0, 2.0]
        )

    def test_absent_source_is_rejected_transactionally(self) -> None:
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

    def test_present_previous_target_is_rejected_as_target(self) -> None:
        scene = Scene()
        first = scene.circle(1.0)
        second = scene.circle(2.0)
        other_source = scene.circle(3.0)
        scene.play(ReplacementTransform(first, second), duration=1.0)
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "target must be absent"):
            scene.play(
                TransformFromCopy(other_source, second),
                duration=1.0,
                start_time=2.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_retroactive_authoring_is_rejected_transactionally(self) -> None:
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

    def test_copy_target_can_become_later_replacement_source(self) -> None:
        scene = Scene()
        source = scene.circle(1.0, key="source")
        middle = scene.circle(2.0, key="middle")
        target = scene.circle(3.0, key="target")

        scene.play(TransformFromCopy(source, middle), duration=1.0)
        scene.play(
            ReplacementTransform(middle, target),
            duration=1.0,
            start_time=1.0,
        )

        events = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == middle.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in events],
            [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ],
        )

    def test_absent_object_can_become_a_later_target(self) -> None:
        scene = Scene()
        first = scene.circle(1.0, key="first")
        second = scene.circle(2.0, key="second")
        third = scene.circle(3.0, key="third")

        scene.play(ReplacementTransform(first, second), duration=1.0)
        scene.play(
            ReplacementTransform(third, first),
            duration=1.0,
            start_time=2.0,
        )

        events = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == first.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in events],
            [
                {"from": True, "to": False},
                {"from": False, "to": True},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in events], [1.0, 3.0]
        )


if __name__ == "__main__":
    unittest.main()
