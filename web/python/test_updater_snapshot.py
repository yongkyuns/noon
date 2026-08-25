import unittest
from types import SimpleNamespace

import _manim_updaters as updaters


def _object(index: int) -> dict:
    return {
        "id": index,
        "geometry": {"circle": {"radius": 0.5}},
        "transform": {
            "translation": {"x": float(index), "y": 0.0},
            "rotation": 0.0,
            "scale": {"x": 1.0, "y": 1.0},
        },
        "style": {
            "fill": {"red": 1.0, "green": 1.0, "blue": 1.0, "alpha": 1.0},
            "stroke": None,
            "stroke_width": 0.0,
            "stroke_join": "round",
            "stroke_cap": "round",
            "opacity": 1.0,
        },
    }


def _frame(count: int) -> dict:
    return {
        "time": 1.0,
        "delta_time": 1.0 / 60.0,
        "objects": [
            {
                "object": item["id"],
                "transform": item["transform"],
                "style": item["style"],
                "presence": True,
                "appearance": 1.0,
                "reveal": 1.0,
                "morph": 0.0,
            }
            for item in (_object(index) for index in range(count))
        ],
        "invocations": [{"callback": 0, "object_indices": list(range(count))}],
    }


class CallbackContextTests(unittest.TestCase):
    def test_large_snapshot_materializes_only_objects_actually_read(self) -> None:
        objects = [_object(index) for index in range(1000)]
        context = updaters._CallbackContext(SimpleNamespace(_objects=objects), _frame(1000))

        self.assertEqual(context._current, {})
        self.assertEqual(context._baseline, {})

        current = context.current_raw(777)
        self.assertEqual(current.transform["translation"]["x"], 777.0)
        self.assertEqual(set(context._current), {777})
        self.assertEqual(set(context._baseline), {777})
        self.assertEqual(context.patch_batch(0).patches, [])

    def test_replace_materializes_baseline_before_recording_mutation(self) -> None:
        objects = [_object(index) for index in range(8)]
        context = updaters._CallbackContext(SimpleNamespace(_objects=objects), _frame(8))
        current = context.current_raw(3)
        moved = type(current)(
            geometry=current.geometry,
            transform={
                **current.transform,
                "translation": {"x": 9.0, "y": -2.0},
            },
            style=current.style,
        )
        context.replace_raw(3, moved)

        batch = context.patch_batch(4)
        self.assertEqual(len(batch.patches), 1)
        self.assertEqual(batch.patches[0]["set_transform"]["object"], 3)
        self.assertEqual(set(context._current), {3})
        self.assertEqual(set(context._baseline), {3})


if __name__ == "__main__":
    unittest.main()
