import unittest

from noon import FadeIn, FadeOut, Scene


class FadeAuthoringTests(unittest.TestCase):
    def test_fade_out_and_back_in_preserves_semantic_opacity(self) -> None:
        scene = Scene()
        obj = scene.circle(1.0, opacity=0.4, key="dot")

        scene.play(FadeOut(obj, key="hide"), duration=2.0, start_time=1.0)
        scene.play(FadeIn(obj, key="show"), duration=2.0, start_time=4.0)

        document = scene.to_document()
        self.assertEqual(document["objects"][0]["style"]["opacity"], 0.4)
        appearance = [
            track for track in document["tracks"] if track["property"] == "appearance"
        ]
        self.assertEqual(
            [track["values"] for track in appearance],
            [
                {"scalar": {"from": 1.0, "to": 0.0}},
                {"scalar": {"from": 0.0, "to": 1.0}},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in appearance], [1.0, 4.0]
        )

        presence = [
            track for track in document["tracks"] if track["property"] == "presence"
        ]
        self.assertEqual(
            [track["values"] for track in presence],
            [
                {"bool": {"from": True, "to": False}},
                {"bool": {"from": False, "to": True}},
            ],
        )
        self.assertEqual(
            [track["timing"]["start_time"] for track in presence], [3.0, 4.0]
        )

    def test_first_fade_in_makes_object_absent_before_animation_start(self) -> None:
        scene = Scene()
        obj = scene.circle(1.0, key="dot")
        scene.play(FadeIn(obj, key="intro"), duration=1.5, start_time=2.0)

        tracks = scene.to_document()["tracks"]
        presence = next(track for track in tracks if track["property"] == "presence")
        appearance = next(track for track in tracks if track["property"] == "appearance")
        self.assertEqual(presence["values"], {"bool": {"from": False, "to": True}})
        self.assertEqual(presence["timing"]["start_time"], 2.0)
        self.assertEqual(
            appearance["values"], {"scalar": {"from": 0.0, "to": 1.0}}
        )
        self.assertEqual(appearance["timing"]["start_time"], 2.0)
        self.assertEqual(appearance["timing"]["duration"], 1.5)

    def test_overlapping_fades_are_rejected_transactionally(self) -> None:
        scene = Scene()
        obj = scene.circle(1.0)
        scene.play(FadeOut(obj), duration=2.0, start_time=1.0)
        before_document = scene.to_document()
        before_identity = scene.identity_document()

        with self.assertRaisesRegex(ValueError, "fade animations for one object must not overlap"):
            scene.play(FadeOut(obj), duration=1.0, start_time=2.0)

        self.assertEqual(scene.to_document(), before_document)
        self.assertEqual(scene.identity_document(), before_identity)

    def test_fade_in_requires_absent_state_after_lifecycle_has_started(self) -> None:
        scene = Scene()
        obj = scene.circle(1.0)
        scene.play(FadeIn(obj), duration=1.0, start_time=1.0)
        before = scene.to_document()

        with self.assertRaisesRegex(ValueError, "fade-in target must be absent"):
            scene.play(FadeIn(obj), duration=1.0, start_time=3.0)

        self.assertEqual(scene.to_document(), before)


if __name__ == "__main__":
    unittest.main()
