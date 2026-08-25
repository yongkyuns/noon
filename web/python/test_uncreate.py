import unittest

from noon import Scene, Square, Uncreate


class UncreateAuthoringTests(unittest.TestCase):
    def test_default_uncreate_reverses_reveal_and_removes_at_exact_end(self) -> None:
        scene = Scene()
        square = scene.add(Square(), key="square")
        scene.play(Uncreate(square), run_time=2.0, easing="smooth")

        tracks = scene.to_document()["tracks"]
        reveal = next(track for track in tracks if track["property"] == "reveal")
        self.assertEqual(reveal["values"]["scalar"], {"from": 1.0, "to": 0.0})
        self.assertEqual(reveal["timing"]["start_time"], 0.0)
        self.assertEqual(reveal["timing"]["duration"], 2.0)
        self.assertEqual(reveal["timing"]["easing"], "smooth")

        removal = next(track for track in tracks if track["property"] == "presence")
        self.assertEqual(removal["values"]["bool"], {"from": True, "to": False})
        self.assertEqual(removal["timing"]["start_time"], 2.0)
        self.assertEqual(removal["timing"]["duration"], 0.0)

    def test_uncreate_can_disable_reverse_and_removal_independently(self) -> None:
        scene = Scene()
        square = scene.add(Square(), key="square")
        scene.play(
            Uncreate(square, reverse_rate_function=False, remover=False),
            run_time=1.0,
            easing="linear",
        )

        tracks = scene.to_document()["tracks"]
        self.assertEqual(len(tracks), 1)
        self.assertEqual(tracks[0]["property"], "reveal")
        self.assertEqual(tracks[0]["values"]["scalar"], {"from": 0.0, "to": 1.0})
        self.assertEqual(tracks[0]["timing"]["easing"], "linear")


if __name__ == "__main__":
    unittest.main()
