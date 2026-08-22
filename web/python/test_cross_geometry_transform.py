import unittest

from noon import BLUE, PURPLE, Circle, Scene, Square, Transform


class CrossGeometryTransformTests(unittest.TestCase):
    def test_circle_to_square_stays_semantic_in_scene_document(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(1.0, color=BLUE), key="circle")

        scene.play(
            Transform(circle, Square(2.0, color=PURPLE), key="circle.to-square"),
            run_time=1.5,
            easing="ease_in_out_cubic",
        )

        document = scene.to_document()
        self.assertEqual(len(document["objects"]), 1)
        self.assertEqual(len(document["tracks"]), 1)
        track = document["tracks"][0]
        self.assertEqual(track["property"], "transform")
        snapshots = track["values"]["object"]
        self.assertEqual(snapshots["from"]["geometry"], {"circle": {"radius": 1.0}})
        self.assertEqual(
            snapshots["to"]["geometry"],
            {"rectangle": {"size": {"x": 2.0, "y": 2.0}}},
        )
        self.assertEqual(snapshots["from"]["style"]["fill"], BLUE.to_ir())
        self.assertEqual(snapshots["to"]["style"]["fill"], PURPLE.to_ir())


if __name__ == "__main__":
    unittest.main()
