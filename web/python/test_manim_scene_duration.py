import unittest

from noon import Scene


class SceneDurationTests(unittest.TestCase):
    def test_default_and_explicit_wait_advance_authored_scene_time(self):
        scene = Scene()

        self.assertEqual(scene.time, 0.0)
        scene.wait()
        self.assertEqual(scene.time, 1.0)
        scene.wait(0.25)
        self.assertEqual(scene.time, 1.25)

    def test_wait_only_duration_is_not_recoverable_from_tracks(self):
        scene = Scene()
        scene.wait(2.5)

        document = scene.to_document()
        self.assertEqual(document["tracks"], [])
        self.assertEqual(scene.time, 2.5)


if __name__ == "__main__":
    unittest.main()
