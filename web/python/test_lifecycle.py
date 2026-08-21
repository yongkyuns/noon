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

    def test_replacement_rejects_foreign_self_and_allows_chained_objects(self) -> None:
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
        scene.play(
            ReplacementTransform(target, third),
            duration=1.0,
            start_time=1.0,
        )

        target_presence = [
            track
            for track in scene.to_document()["tracks"]
            if track["object"] == target.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in target_presence],
            [
                {"from": False, "to": True},
                {"from": True, "to": False},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in target_presence],
            [1.0, 2.0],
        )

    def test_replacement_evaluates_target_property_state_at_handoff(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(1.0)
        scene.animate_position(
            target,
            (0.0, 0.0),
            (3.0, 0.0),
            duration=3.0,
            start_time=0.0,
        )

        scene.play(
            ReplacementTransform(source, target),
            duration=1.0,
            start_time=1.0,
        )

        transform = next(
            track
            for track in scene.to_document()["tracks"]
            if track["property"] == "transform"
        )
        self.assertEqual(
            transform["values"]["object"]["to"]["transform"]["translation"],
            {"x": 2.0, "y": 0.0},
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

    def test_transform_from_copy_rejects_foreign_self_and_allows_valid_reuse(self) -> None:
        scene = Scene()
        source = scene.circle(1.0, key="source")
        target = scene.circle(2.0, key="target")
        foreign = Scene().circle(3.0)

        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(source, foreign), duration=1.0)
        with self.assertRaises(ValueError):
            scene.play(TransformFromCopy(source, source), duration=1.0)

        scene.play(TransformFromCopy(source, target, key="first"), duration=1.0)
        third = scene.circle(4.0, key="third")
        fourth = scene.circle(5.0, key="fourth")
        scene.play(
            TransformFromCopy(source, third, key="second"),
            duration=1.0,
            start_time=2.0,
        )
        scene.play(
            TransformFromCopy(target, fourth, key="third-copy"),
            duration=1.0,
            start_time=2.0,
        )

        document = scene.to_document()
        self.assertEqual(len(document["objects"]), 7)
        target_presence = [
            track
            for track in document["tracks"]
            if track["object"] == target.id and track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"]["bool"] for track in target_presence],
            [{"from": False, "to": True}],
        )

    def test_transform_from_copy_evaluates_source_and_target_property_state(self) -> None:
        scene = Scene()
        source = scene.circle(1.0)
        target = scene.circle(2.0)
        scene.animate_position(
            source,
            (0.0, 0.0),
            (2.0, 0.0),
            duration=2.0,
            start_time=0.0,
        )
        scene.animate_opacity(
            target,
            1.0,
            0.5,
            duration=2.0,
            start_time=1.0,
        )

        scene.play(
            TransformFromCopy(source, target),
            duration=1.0,
            start_time=1.0,
        )

        document = scene.to_document()
        copy_snapshot = {
            key: value for key, value in document["objects"][2].items() if key != "id"
        }
        self.assertEqual(
            copy_snapshot["transform"]["translation"], {"x": 1.0, "y": 0.0}
        )
        transform = next(
            track
            for track in document["tracks"]
            if track["property"] == "transform" and track["object"] == 2
        )
        self.assertEqual(transform["values"]["object"]["to"]["style"]["opacity"], 0.75)

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


class TransactionalPlayTests(unittest.TestCase):
    def test_transform_from_copy_failure_rolls_back_transient_object_and_ids(self) -> None:
        scene = Scene()
        source = scene.circle(1.0, key="source")
        target = scene.circle(2.0, key="target")
        scene.animate_opacity(
            source,
            1.0,
            0.5,
            duration=1.0,
            start_time=5.0,
            key="copy-to-target",
        )
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaises(ValueError):
            scene.play(
                TransformFromCopy(source, target, key="copy-to-target"),
                duration=1.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

        scene.play(
            TransformFromCopy(source, target, key="valid-copy"),
            duration=1.0,
        )
        identities = scene.identity_document()
        self.assertEqual(identities["objects"][-1], {"id": 2, "key": "valid-copy.copy"})
        self.assertEqual(identities["tracks"][1], {"id": 1, "key": "valid-copy"})

    def test_multi_animation_play_rolls_back_tracks_and_scheduled_snapshots(self) -> None:
        scene = Scene()
        first = scene.circle(1.0, key="first")
        second = scene.circle(1.5, key="second")
        scene.animate_opacity(
            second,
            1.0,
            0.5,
            duration=1.0,
            start_time=5.0,
            key="taken",
        )
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaises(ValueError):
            scene.play(
                Transform(first, Circle(2.0), key="would-leak"),
                Transform(second, Circle(3.0), key="taken"),
                duration=1.0,
            )

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

        scene.play(Transform(first, Circle(4.0), key="after"), duration=1.0)
        transform = scene.to_document()["tracks"][1]
        self.assertEqual(
            transform["values"]["object"]["from"]["geometry"],
            {"circle": {"radius": 1.0}},
        )
        self.assertEqual(scene.identity_document()["tracks"][1]["key"], "after")


if __name__ == "__main__":
    unittest.main()
