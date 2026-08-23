import unittest

from noon import BLUE, LEFT, RIGHT, Circle, Create, FadeOut, Line, Scene, Square


class CreateAuthoringTests(unittest.TestCase):
    def test_create_lowers_to_presence_and_reveal_without_rewriting_geometry(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(0.75).set_fill(None).set_stroke(BLUE, 0.08))

        scene.play(Create(circle), run_time=2.0, easing="ease_in_out_cubic")
        document = scene.to_document()

        self.assertIn("circle", document["objects"][0]["geometry"])
        self.assertEqual(
            [track["property"] for track in document["tracks"]],
            ["presence", "reveal"],
        )
        presence, reveal = document["tracks"]
        self.assertEqual(presence["values"]["bool"], {"from": False, "to": True})
        self.assertEqual(reveal["values"]["scalar"], {"from": 0.0, "to": 1.0})
        self.assertEqual(reveal["timing"]["duration"], 2.0)
        self.assertEqual(scene.time, 2.0)

    def test_parallel_create_supports_every_current_shape_kind(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(0.5).set_fill(None).set_stroke(BLUE, 0.06))
        square = scene.add(Square(1.0).set_fill(None).set_stroke(BLUE, 0.06))
        line = scene.add(Line(LEFT, RIGHT).set_stroke(BLUE, 0.06))

        scene.play(Create(circle), Create(square), Create(line), run_time=1.5)
        document = scene.to_document()
        self.assertEqual(
            [track["property"] for track in document["tracks"]].count("presence"),
            3,
        )
        self.assertEqual(
            [track["property"] for track in document["tracks"]].count("reveal"),
            3,
        )
        self.assertTrue(all(track["timing"]["start_time"] == 0.0 for track in document["tracks"]))

    def test_create_rejects_redrawing_a_present_object(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(0.5))
        scene.play(Create(circle), run_time=0.5)

        with self.assertRaisesRegex(ValueError, "must be absent"):
            scene.play(Create(circle), run_time=0.5)

    def test_create_after_fadeout_resets_appearance(self) -> None:
        scene = Scene()
        circle = scene.add(Circle(0.5).set_fill(None).set_stroke(BLUE, 0.06))
        scene.play(Create(circle), run_time=0.5)
        scene.play(FadeOut(circle), run_time=0.5)
        scene.play(Create(circle), run_time=0.5)

        document = scene.to_document()
        properties = [track["property"] for track in document["tracks"]]
        self.assertEqual(properties.count("reveal"), 2)
        self.assertEqual(properties.count("appearance"), 2)


if __name__ == "__main__":
    unittest.main()
