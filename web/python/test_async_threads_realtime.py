"""Check the tutorial's numerical claims without substituting a Noon renderer."""

import ast
from pathlib import Path
import unittest


SOURCE = Path(__file__).parent / "examples" / "async_threads_realtime.py"
TREE = ast.parse(SOURCE.read_text(encoding="utf-8"), filename=str(SOURCE))


def domain_values():
    """Compile only the scene's pure numerical inputs and functions.

    No scene, mobject, timing, membership, or renderer API is mocked here.
    Actual visual and authored-time evidence must come from the Noon host.
    """
    names = {"PERIOD_US", "DEADLINE_US", "CONTROL_US", "DISPATCH_US", "AXIS_US",
             "SCENARIOS", "CHAPTERS"}
    nodes = [node for node in TREE.body if isinstance(node, ast.Assign)
             and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name)
             and node.targets[0].id in names]
    nodes += [node for node in TREE.body if isinstance(node, ast.FunctionDef)
              and node.name == "response_example"]
    namespace = {}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), str(SOURCE), "exec"), namespace)
    return namespace


class AsyncThreadsNumericalClaims(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.values = domain_values()

    def test_completion_margin_and_contiguous_intervals(self):
        expected = {
            "long": (505, 585, -335, False),
            "bounded": (55, 135, 115, True),
            "preemptive": (15, 95, 155, True),
        }
        for name, blocking in self.values["SCENARIOS"]:
            with self.subTest(scenario=name):
                result = self.values["response_example"](blocking)
                start, finish, margin, met = expected[name]
                self.assertEqual(result["blocking"], (0, blocking))
                self.assertEqual(result["dispatch"], (blocking, start))
                self.assertEqual(result["control"], (start, finish))
                self.assertEqual(result["finish"], finish)
                self.assertEqual(result["margin"], margin)
                self.assertEqual(result["met"], met)
                self.assertLess(finish, self.values["AXIS_US"])
                self.assertLess(self.values["AXIS_US"], self.values["PERIOD_US"])

    def test_finishing_exactly_at_deadline_meets_it(self):
        bound = self.values["DEADLINE_US"] - self.values["CONTROL_US"] - self.values["DISPATCH_US"]
        at_deadline = self.values["response_example"](bound)
        late = self.values["response_example"](bound + 1)
        self.assertTrue(at_deadline["met"])
        self.assertEqual(at_deadline["margin"], 0)
        self.assertFalse(late["met"])
        self.assertEqual(late["margin"], -1)

    def test_intervals_preserve_the_given_execution_and_dispatch_budgets(self):
        # Geometry is now owned by native NumberLine.n2p. These checks concern
        # the lesson's time budgets, not a reimplementation of that mapping.
        for _, blocking in self.values["SCENARIOS"]:
            result = self.values["response_example"](blocking)
            durations = [end - start for start, end in
                         (result["blocking"], result["dispatch"], result["control"])]
            self.assertEqual(durations, [blocking, self.values["DISPATCH_US"], self.values["CONTROL_US"]])
            self.assertEqual(sum(durations), result["finish"])
            self.assertAlmostEqual(durations[1] / durations[2], 5 / 80)

    def test_complete_chapter_cue_sheet_and_entry_points(self):
        chapters = self.values["CHAPTERS"]
        scene = next(node for node in TREE.body if isinstance(node, ast.ClassDef))
        methods = {node.name for node in scene.body if isinstance(node, ast.AsyncFunctionDef)}
        self.assertEqual(len(chapters), 13)
        self.assertEqual(sum(duration for _, duration, _ in chapters), 522)
        self.assertTrue(all(duration > 0 and method in methods for _, duration, method in chapters))


if __name__ == "__main__":
    unittest.main()
