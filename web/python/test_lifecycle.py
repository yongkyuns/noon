import unittest

from noon import (
    Circle,
    Color,
    ReplacementTransform,
    Scene,
    Transform,
    TransformFromCopy,
)


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


class TransformFromCopyTests(unittest.TestCase):
    def _build_scene(self) -> Scene:
        scene = Scene()
        source = scene.circle(
            1.0,
            position=(-2.0, 0.0),
            fill=Color(0.9, 0.2, 0.2),
            key="source",
        )
        target = scene.circle(
            3.0,
            position=(4.0, -2.0),
            fill=Color(0.2, 0.5, 0.9),
            key="target",
        )
        scene.play(
            TransformFromCopy(source, target, key="copy-to-target"),
            duration=2.0,
            start_time=1.0,
            easing="ease_in_out_cubic",
        )
        return scene

    def test_transform_from_copy_lowers_to_stable_copy_and_presence_window(self) -> None:
        scene = self._build_scene()
        document = scene.to_document()

        self.assertEqual([obj["id"] for obj in document["objects"]], [0, 1, 2])
        self.assertEqual(
            [track["property"] for track in document["tracks"]],
            ["transform", "presence", "presence", "presence"],
        )

        source_snapshot = {
            key: value for key, value in document["objects"][0].items() if key != "id"
        }
        target_snapshot = {
            key: value for key, value in document["objects"][1].items() if key != "id"
        }
        copy_snapshot = {
            key: value for key, value in document["objects"][2].items() if key != "id"
        }
        self.assertEqual(copy_snapshot, source_snapshot)

        transform, copy_show, copy_hide, target_show = document["tracks"]
        self.assertEqual(transform["object"], 2)
        self.assertEqual(transform["values"]["object"]["from"], source_snapshot)
        self.assertEqual(transform["values"]["object"]["to"], target_snapshot)
        self.assertEqual(
            transform["timing"],
            {
                "start_time": 1.0,
                "duration": 2.0,
                "easing": "ease_in_out_cubic",
            },
        )

        self.assertEqual(copy_show["object"], 2)
        self.assertEqual(copy_show["values"]["bool"], {"from": False, "to": True})
        self.assertEqual(copy_show["timing"]["start_time"], 1.0)
        self.assertEqual(copy_show["timing"]["duration"], 0.0)

        self.assertEqual(copy_hide["object"], 2)
        self.assertEqual(copy_hide["values"]["bool"], {"from": True, "to": False})
        self.assertEqual(copy_hide["timing"]["start_time"], 3.0)
        self.assertEqual(copy_hide["timing"]["duration"], 0.0)

        self.assertEqual(target_show["object"], 1)
        self.assertEqual(target_show["values"]["bool"], {"from": False, "to": True})
        self.assertEqual(target_show["timing"]["start_time"], 3.0)
        self.assertEqual(target_show["timing"]["duration"], 0.0)

        identities = scene.identity_document()
        self.assertEqual(
            identities["objects"],
            [
                {"id": 0, "key": "source"},
                {"id": 1, "key": "target"},
                {"id": 2, "key": "copy-to-target.copy"},
            ],
        )
        self.assertEqual(
            identities["tracks"],
            [
                {"id": 0, "key": "copy-to-target"},
                {"id": 1, "key": "copy-to-target.copy.show"},
                {"id": 2, "key": "copy-to-target.copy.hide"},
                {"id": 3, "key": "copy-to-target.copy.target-show"},
            ],
        )

    def test_transform_from_copy_identity_is_deterministic_across_reruns(self) -> None:
        self.assertEqual(
            self._build_scene().identity_document(),
            self._build_scene().identity_document(),
        )

    def test_transform_from_copy_default_identity_is_deterministic(self) -> None:
        def build() -> Scene:
            scene = Scene()
            source = scene.circle(1.0, key="source")
            target = scene.circle(2.0, key="target")
            scene.play(TransformFromCopy(source, target), duration=1.0)
            return scene

        identities = build().identity_document()
        self.assertEqual(identities, build().identity_document())
        self.assertEqual(identities["objects"][2]["key"], "@copy:source->target")
        self.assertEqual(
            [track["key"] for track in identities["tracks"]],
            [
                "@copy:source->target.transform",
                "@copy:source->target.show",
                "@copy:source->target.hide",
                "@copy:source->target.target-show",
            ],
        )

    def test_transform_from_copy_rejects_foreign_self_and_reused_objects(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        foreign = Scene().circle(3.0)

        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(source, foreign), duration=1.0)
        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(source, source), duration=1.0)

        scene.play(TransformFromCopy(source, target), duration=1.0)
        third = scene.circle(4.0)
        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(source, third), duration=1.0, start_time=2.0)
        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(target, third), duration=1.0, start_time=2.0)

    def test_transform_from_copy_rejects_ambiguous_source_or_target_state(self) -> None:
        source_scene = Scene()
        source = source_scene.circle(1.0)
        target = source_scene.circle(2.0)
        source_scene.animate_position(
            source,
            (0.0, 0.0),
            (1.0, 0.0),
            duration=1.0,
            start_time=0.0,
        )
        with self.assertRaises(ValueError):
            source_scene.play(
                TransformFromCopy(source, target),
                duration=1.0,
                start_time=1.0,
            )

        target_scene = Scene()
        source = target_scene.circle(1.0)
        target = target_scene.circle(2.0)
        target_scene.animate_opacity(
            target,
            1.0,
            0.5,
            duration=1.0,
            start_time=2.0,
        )
        with self.assertRaises(ValueError):
            target_scene.play(
                TransformFromCopy(source, target),
                duration=1.0,
                start_time=1.0,
            )

    def test_transform_from_copy_rejects_overlapping_source_or_target_transform(self) -> None:
        source_scene = Scene()
        source = source_scene.circle(1.0)
        target = source_scene.circle(2.0)
        source_scene.play(Transform(source, Circle(1.5)), duration=3.0)
        with self.assertRaises(ValueError):
            source_scene.play(
                TransformFromCopy(source, target),
                duration=1.0,
                start_time=1.0,
            )

        target_scene = Scene()
        source = target_scene.circle(1.0)
        target = target_scene.circle(2.0)
        target_scene.play(Transform(target, Circle(2.5)), duration=3.0)
        with self.assertRaises(ValueError):
            target_scene.play(
                TransformFromCopy(source, target),
                duration=1.0,
                start_time=1.0,
            )


if __name__ == "__main__":
    unittest.main()
