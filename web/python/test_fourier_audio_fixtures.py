"""Source/numerical regression fixtures for #2358, not renderer/audio proof.

Source contracts use only the standard library. Numerical tests execute the
actual example's pure preparation functions when NumPy is installed. They do
not emulate Noon, call Scene.construct(), or claim browser/native playback.
"""
import ast
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import unittest

try:
    import numpy as np
except ImportError:
    np = None


FIXTURES = Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "fourier_audio"


def source(name):
    return ast.parse((FIXTURES / name).read_text(encoding="utf-8"), filename=name)


def constants(tree):
    result = {}
    for statement in tree.body:
        if isinstance(statement, ast.Assign) and len(statement.targets) == 1:
            target = statement.targets[0]
            if isinstance(target, ast.Name):
                result[target.id] = ast.literal_eval(statement.value)
    return result


def construct(tree):
    classes = [node for node in tree.body if isinstance(node, ast.ClassDef)]
    if len(classes) != 1:
        raise AssertionError("each fixture must contain one runnable Scene")
    return next(node for node in classes[0].body
                if isinstance(node, ast.FunctionDef) and node.name == "construct")


def named_calls(node, name):
    return [call for call in ast.walk(node) if isinstance(call, ast.Call)
            and isinstance(call.func, ast.Attribute) and call.func.attr == name]


def duration(call, values):
    expression = (call.args[0] if call.func.attr == "wait" else
                  next(keyword.value for keyword in call.keywords if keyword.arg == "run_time"))
    value = values[expression.id] if isinstance(expression, ast.Name) else ast.literal_eval(expression)
    return Fraction(str(value))


def preparation():
    # Load the source's mathematical functions, not a rewritten reference model
    # or a fake Noon runtime. All other AST nodes remain unexecuted.
    tree = source("fourier_epicycles.py")
    names = {"resample_outline", "fourier_model", "prepare_positions", "xy"}
    functions = [node for node in tree.body
                 if isinstance(node, ast.FunctionDef) and node.name in names]
    if {node.name for node in functions} != names:
        raise AssertionError("the baseline preparation functions changed")
    namespace = {"np": np, **constants(tree)}
    exec(compile(ast.Module(body=functions, type_ignores=[]), "fourier_epicycles.py", "exec"), namespace)
    return namespace


class FourierAudioSourceTests(unittest.TestCase):
    def test_preserved_baselines_match_recorded_provenance_and_parse(self):
        manifest = json.loads((FIXTURES / "provenance.json").read_text(encoding="utf-8"))
        self.assertEqual(len(manifest["fixtures"]), 3)
        for fixture in manifest["fixtures"]:
            with self.subTest(path=fixture["path"]):
                data = (FIXTURES / fixture["path"]).read_bytes()
                self.assertEqual(len(data), fixture["bytes"])
                self.assertEqual(hashlib.sha256(data).hexdigest(), fixture["sha256"])
                construct(ast.parse(data))

    def test_music_duration_matches_every_fourier_choreography_segment(self):
        tree = source("fourier_epicycles.py")
        values = constants(tree)
        body = construct(tree)
        loops = [node for node in body.body if isinstance(node, ast.For)
                 and named_calls(node, "play")]
        self.assertEqual(len(loops), 1)
        loop = loops[0]
        direct = [node.value for node in body.body if isinstance(node, ast.Expr)
                  and isinstance(node.value, ast.Call)
                  and isinstance(node.value.func, ast.Attribute)
                  and node.value.func.attr in {"play", "wait"}]
        per_stage = [node.value for node in loop.body if isinstance(node, ast.Expr)
                     and isinstance(node.value, ast.Call)
                     and isinstance(node.value.func, ast.Attribute)
                     and node.value.func.attr in {"play", "wait"}]
        transitions = [call for node in loop.body if isinstance(node, ast.If)
                       for call in named_calls(node, "play")]
        self.assertEqual((len(direct), len(per_stage), len(transitions)), (3, 3, 1))
        count = len(values["TERM_COUNTS"])
        total = (sum(duration(call, values) for call in direct)
                 + count * sum(duration(call, values) for call in per_stage)
                 + (count - 1) * sum(duration(call, values) for call in transitions))
        self.assertEqual(total, Fraction("34.65"))
        music = constants(source("music_preview.py"))
        self.assertEqual(total, Fraction(str(music["DURATION"])))

    def test_baselines_have_no_python_frame_callbacks_or_path_replacement(self):
        forbidden = {"add_updater", "set_effective_path", "always_redraw"}
        for name in ("fourier_epicycles.py", "music_preview.py"):
            tree = source(name)
            for node in ast.walk(tree):
                if isinstance(node, ast.Call):
                    called = node.func.attr if isinstance(node.func, ast.Attribute) else getattr(node.func, "id", None)
                    self.assertNotIn(called, forbidden, (name, called))
        self.assertTrue(any(isinstance(node, ast.Import) and
                            any(alias.name == "numpy" for alias in node.names)
                            for node in source("fourier_epicycles.py").body))

    def test_finite_preview_declares_membership_before_native_timing(self):
        body = construct(source("music_preview.py"))
        additions = named_calls(body, "add")
        declarations = named_calls(body, "declare_value")
        self.assertEqual(len(declarations), 1)
        self.assertTrue(additions)
        self.assertLess(max(call.lineno for call in additions), declarations[0].lineno)
        self.assertEqual(len(named_calls(body, "audio")), 1)
        self.assertEqual(named_calls(body, "play"), [])
        self.assertEqual(named_calls(body, "wait"), [])

    def test_blocked_combination_is_still_an_ordinary_play_wait_scene(self):
        body = construct(source("audio_ordinary_play.py"))
        audio = named_calls(body, "audio")
        plays = named_calls(body, "play")
        waits = named_calls(body, "wait")
        self.assertEqual((len(audio), len(plays), len(waits)), (1, 1, 1))
        self.assertLess(audio[0].lineno, plays[0].lineno)
        self.assertLess(plays[0].lineno, waits[0].lineno)
        self.assertEqual(duration(plays[0], {}) + duration(waits[0], {}), 3)
        self.assertEqual(named_calls(body, "declare_value"), [])


@unittest.skipIf(np is None, "NumPy is required for numerical preparation checks")
class FourierPreparationTests(unittest.TestCase):
    def setUp(self):
        self.api = preparation()

    def model(self, dense=False):
        phase = np.arange(128) / 128
        samples = (1.2 + 0.3j + 0.9 * np.exp(2j * np.pi * phase)
                   + 0.35j * np.exp(-4j * np.pi * phase))
        if dense:
            for k in range(1, 31):
                samples += (0.08 / k ** 1.3) * np.exp(1j * k * 0.37) * np.exp(2j * np.pi * k * phase)
                samples += (0.06j / k ** 1.2) * np.exp(-2j * np.pi * k * phase)
        return self.api["fourier_model"](samples, (7, 21, 61))

    def test_fft_sign_normalization_and_constant_offset(self):
        frequencies, coefficients, offset, samples = self.model()
        spectrum = np.fft.fft(samples) / len(samples)
        np.testing.assert_array_equal(frequencies[:6], [1, -1, 2, -2, 3, -3])
        np.testing.assert_allclose(coefficients, spectrum[frequencies % len(samples)], atol=1e-12)
        np.testing.assert_allclose(offset, spectrum[0], atol=1e-12)
        phase = np.arange(len(samples)) / len(samples)
        reconstructed = offset + np.exp(2j * np.pi * np.outer(phase, frequencies)) @ coefficients
        np.testing.assert_allclose(reconstructed, samples, atol=1e-12)

    def test_open_outline_retraces_instead_of_adding_a_closing_chord(self):
        sampled = self.api["resample_outline"]([0j, 1 + 0j, 1 + 1j], 8)
        np.testing.assert_allclose(sampled, [0j, 0.5, 1, 1 + 0.5j, 1 + 1j, 1 + 0.5j, 1, 0.5])

    def test_duplicate_points_preserve_closed_outline(self):
        base = [0j, 1 + 0j, 1 + 1j, 1j, 0j]
        duplicate = [0j, 0j, 1 + 0j, 1 + 0j, 1 + 1j, 1j, 0j, 0j]
        np.testing.assert_allclose(self.api["resample_outline"](base, 64),
                                   self.api["resample_outline"](duplicate, 64))

    def test_degenerate_and_nonfinite_outlines_reject(self):
        for points in ([], [0j], [1j, 1j], [0j, complex(float("nan"), 1)]):
            with self.subTest(points=points), self.assertRaises(ValueError):
                self.api["resample_outline"](points, 64)

    def test_invalid_term_sequences_reject(self):
        samples = np.exp(2j * np.pi * np.arange(64) / 64)
        for counts in ((), (2,), (4,), (7, 3), (7, 7), (65,)):
            with self.subTest(counts=counts), self.assertRaises(ValueError):
                self.api["fourier_model"](samples, counts)

    def test_adaptive_interpolation_obeys_declared_error_for_every_joint(self):
        frequencies, coefficients, offset, _ = self.model(dense=True)
        for terms in (7, 21, 61):
            count = terms - 1
            keys, chain, bound = self.api["prepare_positions"](frequencies, coefficients, offset, count)
            self.assertLessEqual(bound, self.api["MAX_POINT_ERROR"])
            self.assertTrue(np.all(np.diff(keys) > 0))
            np.testing.assert_array_equal(chain[0], chain[-1])
            phase = (np.arange(2048) + 0.37) / 2048
            rotating = np.exp(2j * np.pi * np.outer(phase, frequencies[:count])) * coefficients[:count]
            exact = offset + np.column_stack((np.zeros(len(phase)), np.cumsum(rotating, axis=1)))
            observed = np.column_stack([np.interp(phase, keys, column.real)
                                        + 1j * np.interp(phase, keys, column.imag) for column in chain.T])
            self.assertLessEqual(float(np.max(np.abs(exact - observed))), bound + 1e-12)

    def test_trace_grid_and_pen_interpolation_agree_at_intermediate_phases(self):
        frequencies, coefficients, offset, _ = self.model(dense=True)
        keys, chain, _ = self.api["prepare_positions"](frequencies, coefficients, offset, 60)
        endpoint = chain[:, -1]
        uniform = np.linspace(0, 1, round(1 / float(np.min(np.diff(keys)))) + 1)
        trace = np.interp(uniform, keys, endpoint.real) + 1j * np.interp(uniform, keys, endpoint.imag)
        phase = np.linspace(0, 1, 1003)
        for component in ("real", "imag"):
            np.testing.assert_allclose(np.interp(phase, uniform, getattr(trace, component)),
                                       np.interp(phase, keys, getattr(endpoint, component)), atol=1e-12)

    def test_invalid_accuracy_budget_rejects(self):
        frequencies, coefficients, offset, _ = self.model()
        for value in (0, -1, float("nan"), float("inf")):
            self.api["MAX_POINT_ERROR"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.api["prepare_positions"](frequencies, coefficients, offset, 6)


if __name__ == "__main__":
    unittest.main()
