import unittest

from noon import Circle, Color, ReplacementTransform, Scene, Transform


class ReplacementTransformTests(unittest.TestCase):
    def test_replacement_lowers_to_transform_and_atomic_presence_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(
            1.0,
            position=(-2.0, 0.0),
            fill=Color(0.9, 0.2, 0.2),
            key="source",
        )
        target = scene.circle(
            1.5,
            position=(2.0, 1.0),
            fill=Color(0.2, 0.5, 0.9),
            key="target",
        )

        scene.play(
            ReplacementTransform(source, target, key="swap"),
            duration=2.0,
            start_time=1.0,
            easing="ease_in_out_cubic",
        )

        document = scene.to_document()
        self.assertEqual([obj["id"] for obj in document["objects"]], [0, 1])
        self.assertEqual(
            [track["property"] for track in document["tracks"]],
            ["transform", "presence", "presence"],
        )

        transform, source_presence, target_presence = document["tracks"]
        self.assertEqual(transform["object"], source.id)
        self.assertEqual(transform["timing"]["start_time"], 1.0)
        self.assertEqual(transform["timing"]["duration"], 2.0)
        self.assertEqual(transform["timing"]["easing"], "ease_in_out_cubic")
        target_snapshot = {
            key: value
            for key, value in document["objects"][target.id].items()
            if key != "id"
        }
        self.assertEqual(transform["values"]["object"]["to"], target_snapshot)

        self.assertEqual(source_presence["object"], source.id)
        self.assertEqual(
            source_presence["values"]["bool"], {"from": True, "to": False}
        )
        self.assertEqual(source_presence["timing"]["start_time"], 3.0)
        self.assertEqual(source_presence["timing"]["duration"], 0.0)

        self.assertEqual(target_presence["object"], target.id)
        self.assertEqual(
            target_presence["values"]["bool"], {"from": False, "to": True}
        )
        self.assertEqual(target_presence["timing"]["start_time"], 3.0)
        self.assertEqual(target_presence["timing"]["duration"], 0.0)

        identities = scene.identity_document()["tracks"]
        self.assertEqual(identities[0], {"id": 0, "key": "swap"})
        self.assertEqual(identities[1]["key"], "@track:1")
        self.assertEqual(identities[2]["key"], "@track:2")

    def test_replacement_rejects_foreign_self_and_reused_lifecycle_objects(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        foreign = Scene().circle(3.0)

        with self.assertRaises(ValueError):
            scene.play(ReplacementTransform(source, foreign), duration=1.0)
        with self.assertRaises(ValueError):
            scene.play(ReplacementTransform(source, source), duration=1.0)

        scene.play(ReplacementTransform(source, target), duration=1.0)
        third = scene.circle(4.0)
        with self.assertRaises(ValueError):
            scene.play(
                ReplacementTransform(target, third),
                duration=1.0,
                start_time=2.0,
            )

    def test_replacement_rejects_target_property_state_before_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(1.0)
        scene.animate_position(
            target,
            (0.0, 0.0),
            (1.0, 0.0),
            duration=3.0,
            start_time=0.0,
        )

        with self.assertRaises(ValueError):
            scene.play(
                ReplacementTransform(source, target),
                duration=1.0,
                start_time=1.0,
            )

    def test_replacement_rejects_target_with_overlapping_transform(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(1.0)
        scene.play(Transform(target, Circle(2.0)), duration=3.0, start_time=0.0)

        with self.assertRaises(ValueError):
            scene.play(
                ReplacementTransform(source, target),
                duration=1.0,
                start_time=1.0,
            )


if __name__ == "__main__":
    unittest.main()
