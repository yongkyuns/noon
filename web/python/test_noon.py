import json
import math
import unittest

from noon import Color, PatchBatch, Scene, VectorPath


class PatchBatchTests(unittest.TestCase):
    def test_patch_batch_matches_versioned_noon_ir_shape(self) -> None:
        batch = (
            PatchBatch(7)
            .set_style(
                2,
                fill=None,
                stroke=Color(0.2, 0.4, 0.6),
                stroke_width=0.1,
            )
            .set_transform(
                3,
                translation=(4.0, -2.0),
                rotation=0.5,
                scale=(2.0, 3.0),
            )
        )

        document = json.loads(batch.to_json())

        self.assertEqual(document["version"], 1)
        self.assertEqual(document["sequence"], 7)
        self.assertEqual(document["patches"][0]["set_style"]["object"], 2)
        self.assertEqual(
            document["patches"][1]["set_transform"]["transform"]["translation"],
            {"x": 4.0, "y": -2.0},
        )

    def test_invalid_identifiers_are_rejected_before_transport(self) -> None:
        with self.assertRaises(ValueError):
            PatchBatch(-1)
        with self.assertRaises(TypeError):
            PatchBatch(0).set_transform(True)

    def test_non_finite_values_are_rejected_before_transport(self) -> None:
        with self.assertRaises(ValueError):
            Color(math.inf, 0.0, 0.0)
        with self.assertRaises(ValueError):
            PatchBatch(0).set_transform(0, rotation=math.nan)


class SceneTests(unittest.TestCase):
    def test_scene_matches_versioned_noon_ir_shape_with_stable_ids(self) -> None:
        scene = Scene()
        circle = scene.circle(
            0.65,
            position=(-2.1, 0.8),
            fill=Color(0.98, 0.38, 0.36),
            stroke=Color(1.0, 1.0, 1.0),
            stroke_width=0.04,
        )
        rectangle = scene.rectangle(1.5, 0.9, rotation=-0.7)
        scene.animate_position(
            circle,
            (-2.1, 0.8),
            (2.1, -0.8),
            duration=4.0,
            easing="ease_in_out_cubic",
        ).animate_rotation(
            rectangle,
            -0.7,
            math.tau - 0.7,
            duration=4.0,
            easing="ease_in_out_cubic",
        )

        document = json.loads(scene.to_json())

        self.assertEqual(document["version"], 1)
        self.assertEqual([obj["id"] for obj in document["objects"]], [0, 1])
        self.assertEqual(
            document["objects"][0]["geometry"], {"circle": {"radius": 0.65}}
        )
        self.assertEqual(
            document["objects"][1]["geometry"]["rectangle"]["size"],
            {"x": 1.5, "y": 0.9},
        )
        self.assertEqual([track["id"] for track in document["tracks"]], [0, 1])
        self.assertEqual(document["tracks"][0]["property"], "position")
        self.assertEqual(document["tracks"][1]["values"]["scalar"]["from"], -0.7)

    def test_path_reveal_serializes_as_normalized_scalar_track(self) -> None:
        scene = Scene()
        path = scene.path(
            VectorPath().move_to((-1.0, 0.0)).line_to((1.0, 0.0)),
            fill=None,
            stroke=Color(1.0, 1.0, 1.0),
            stroke_width=0.08,
            key="stroke",
        )
        scene.animate_reveal(
            path,
            duration=2.5,
            start_time=0.75,
            easing="ease_in_out_cubic",
            key="stroke.reveal",
        )

        track = scene.to_document()["tracks"][0]
        self.assertEqual(track["property"], "reveal")
        self.assertEqual(track["values"]["scalar"], {"from": 0.0, "to": 1.0})
        self.assertEqual(track["timing"]["start_time"], 0.75)
        self.assertEqual(track["timing"]["duration"], 2.5)
        self.assertEqual(track["timing"]["easing"], "ease_in_out_cubic")

        with self.assertRaises(ValueError):
            scene.animate_reveal(path, from_=-0.1, duration=1.0)
        with self.assertRaises(ValueError):
            scene.animate_reveal(path, to=1.1, duration=1.0)

    def test_path_morph_serializes_target_and_normalized_progress(self) -> None:
        scene = Scene()
        source = VectorPath().move_to((-1.0, 0.0)).line_to((1.0, 0.0))
        target = VectorPath().move_to((0.0, -1.0)).line_to((0.0, 1.0))
        path = scene.path(
            source,
            fill=None,
            stroke=Color(1.0, 1.0, 1.0),
            stroke_width=0.1,
            key="morph",
        )
        scene.animate_morph(
            path,
            target,
            duration=3.0,
            start_time=0.5,
            easing="ease_in_out_cubic",
            key="morph.shape",
        )

        document = scene.to_document()
        vector_path = document["objects"][0]["geometry"]["vector_path"]
        self.assertEqual(
            vector_path["morph_target"]["commands"][0]["move_to"]["to"],
            {"x": 0.0, "y": -1.0},
        )
        track = document["tracks"][0]
        self.assertEqual(track["property"], "reveal")
        self.assertEqual(track["values"]["scalar"], {"from": 0.0, "to": 1.0})

        with self.assertRaises(ValueError):
            scene.animate_reveal(path, duration=1.0)

    def test_scene_rejects_foreign_objects_and_invalid_timing(self) -> None:
        first = Scene()
        second = Scene()
        circle = first.circle(1.0)

        with self.assertRaises(ValueError):
            second.animate_position(circle, (0.0, 0.0), (1.0, 1.0), duration=1.0)
        with self.assertRaises(ValueError):
            first.animate_rotation(circle, 0.0, 1.0, duration=0.0)
        with self.assertRaises(ValueError):
            first.animate_opacity(
                circle, 0.0, 1.0, duration=1.0, easing="spring"
            )

    def test_scene_rejects_invalid_geometry_before_transport(self) -> None:
        with self.assertRaises(ValueError):
            Scene().circle(0.0)
        with self.assertRaises(ValueError):
            Scene().rectangle(1.0, math.nan)

    def test_vector_path_serializes_curves_and_close_commands(self) -> None:
        path = (
            VectorPath()
            .move_to((-1.0, 0.0))
            .quadratic_to((0.0, 2.0), (1.0, 0.0))
            .cubic_to((1.0, -1.0), (-1.0, -1.0), (-1.0, 0.0))
            .close()
        )
        scene = Scene()
        scene.path(
            path,
            fill=Color(0.5, 0.2, 0.9),
            stroke=Color(1.0, 1.0, 1.0),
            stroke_width=0.08,
            key="curve",
        )

        geometry = json.loads(scene.to_json())["objects"][0]["geometry"]
        self.assertEqual(
            geometry["vector_path"]["commands"][0]["move_to"]["to"],
            {"x": -1.0, "y": 0.0},
        )
        self.assertEqual(geometry["vector_path"]["commands"][-1], "close")

        with self.assertRaises(TypeError):
            Scene().path(object())  # type: ignore[arg-type]

    def test_scene_exports_stable_explicit_authoring_keys(self) -> None:
        scene = Scene()
        hero = scene.circle(1.0, key="hero")
        scene.animate_opacity(hero, 0.0, 1.0, duration=1.0, key="hero.fade")

        self.assertEqual(
            scene.identity_document(),
            {
                "objects": [{"id": 0, "key": "hero"}],
                "tracks": [{"id": 0, "key": "hero.fade"}],
            },
        )
        with self.assertRaises(ValueError):
            scene.rectangle(1.0, 1.0, key="hero")


if __name__ == "__main__":
    unittest.main()
