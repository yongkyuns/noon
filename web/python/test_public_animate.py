import unittest

from noon import BLUE, PURPLE, RIGHT, UP, Circle, Scene


class PublicAnimateTests(unittest.TestCase):
    def test_sequential_animate_starts_from_prior_semantic_endpoint(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(0.5, color=BLUE), key="circle")

        scene.play(
            circle.animate.shift(RIGHT).set_color(PURPLE),
            run_time=1.0,
            easing="ease_in_out_cubic",
        )
        scene.play(circle.animate.shift(UP), run_time=1.0)

        tracks = scene.to_document()["tracks"]
        self.assertEqual([track["property"] for track in tracks], ["transform", "transform"])
        first_target = tracks[0]["values"]["object"]["to"]
        second_source = tracks[1]["values"]["object"]["from"]
        second_target = tracks[1]["values"]["object"]["to"]

        self.assertEqual(second_source, first_target)
        self.assertEqual(second_source["transform"]["translation"], {"x": 1.0, "y": 0.0})
        self.assertEqual(second_target["transform"]["translation"], {"x": 1.0, "y": 1.0})
        self.assertEqual(second_target["style"]["fill"], PURPLE.to_ir())
        self.assertEqual(circle.get_center(), RIGHT + UP)
        self.assertEqual(scene.time, 2.0)


if __name__ == "__main__":
    unittest.main()
