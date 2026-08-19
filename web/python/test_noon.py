import json
import math
import unittest

from noon import Color, PatchBatch


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


if __name__ == "__main__":
    unittest.main()
