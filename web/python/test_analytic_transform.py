import unittest

from noon import Circle, Color, Line, Rectangle, Scene, Transform


class AnalyticTransformTests(unittest.TestCase):
    def test_detached_analytic_targets_serialize_as_atomic_transforms(self) -> None:
        scene = Scene()
        circle = scene.circle(1.0, key="circle")
        rectangle = scene.rectangle(2.0, 3.0, key="rectangle")
        line = scene.line((-1.0, 0.0), (1.0, 0.0), key="line")

        scene.play(
            Transform(circle, Circle(3.0, position=(2.0, -1.0), opacity=0.5)),
            Transform(rectangle, Rectangle(6.0, 8.0, rotation=0.4)),
            Transform(
                line,
                Line(
                    (0.0, -2.0),
                    (0.0, 2.0),
                    stroke=Color(0.2, 0.7, 0.9),
                ),
            ),
            duration=2.0,
        )

        document = scene.to_document()
        self.assertEqual(len(document["objects"]), 3)
        self.assertEqual([track["property"] for track in document["tracks"]], ["transform"] * 3)
        self.assertEqual(
            document["tracks"][0]["values"]["object"]["to"]["geometry"],
            {"circle": {"radius": 3.0}},
        )
        self.assertEqual(
            document["tracks"][1]["values"]["object"]["to"]["geometry"]["rectangle"]["size"],
            {"x": 6.0, "y": 8.0},
        )
        self.assertEqual(
            document["tracks"][2]["values"]["object"]["to"]["geometry"]["line"],
            {
                "start": {"x": 0.0, "y": -2.0},
                "end": {"x": 0.0, "y": 2.0},
            },
        )


if __name__ == "__main__":
    unittest.main()
