"""Domain, lexical, source-order and authored-geometry checks.

These tests do NOT qualify rendering/playback. Small authoring helpers use
unittest.mock call spies to inspect arguments; no Noon runtime is emulated.
"""
import ast
import json
import re
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, call
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCENE_PATH = ROOT / "web/python/examples/cuda_transpose_banks.py"
SOURCE = SCENE_PATH.read_text(encoding="utf-8")
TREE = ast.parse(SOURCE)
PURE = {"reference_trace", "get_trace", "service_batches", "tile_point",
        "queue_point", "bank_center", "bank_point", "register_point",
        "output_point", "syntax_spans", "code_entries"}
CONSTANTS = {"TILE_LEFT", "TILE_TOP", "CELL", "QUEUE_W", "QUEUE_H",
             "REGISTER_SCALE", "REGISTER_CELL_W", "REGISTER_CELL_H",
             "ROUND_IN", "ROUND_OUT", "SOURCE_STEPS", "CODE_TOP", "CODE_STEP", "CODE_LINE_H"}
NS = {"GPU_TRACE": None, "COLUMN": 3, "re": re}
nodes = []
for n in TREE.body:
    if isinstance(n, ast.FunctionDef) and n.name in PURE:
        nodes.append(n)
    if isinstance(n, ast.Assign):
        ids = {x.id for t in n.targets for x in ast.walk(t) if isinstance(x, ast.Name)}
        if ids and ids <= CONSTANTS:
            nodes.append(n)
exec(compile(ast.Module(body=nodes, type_ignores=[]), "<pure authored inputs>", "exec"), NS)
reference, service, get_trace = (NS[n] for n in ("reference_trace", "service_batches", "get_trace"))


def separated(a, b, w, h):
    return abs(a[0] - b[0]) >= w - 1e-10 or abs(a[1] - b[1]) >= h - 1e-10


def assert_nonoverlap(case, points, w, h):
    for i, a in enumerate(points):
        for j, b in enumerate(points[i + 1:], i + 1):
            case.assertTrue(separated(a, b, w, h), (i, j, a, b, w, h))


def assert_inside(case, points, w, h, bounds):
    xmin, ymin, xmax, ymax = bounds
    for p in points:
        case.assertGreaterEqual(p[0] - w / 2, xmin - 1e-9)
        case.assertGreaterEqual(p[1] - h / 2, ymin - 1e-9)
        case.assertLessEqual(p[0] + w / 2, xmax + 1e-9)
        case.assertLessEqual(p[1] + h / 2, ymax + 1e-9)
class TraceTests(unittest.TestCase):
    def setUp(self):
        NS.update(GPU_TRACE=None, COLUMN=3)

    def test_all_columns_both_layouts_and_base_offsets(self):
        for base in (0, 1, 31, 32, 128):
            for column in range(32):
                for stride in (32, 33):
                    with self.subTest(base=base, column=column, stride=stride):
                        trace = reference(stride, column, base)
                        self.assertEqual(trace['loads'], [256] * 4)
                        self.assertEqual(trace['verified_output_words'], 1024)
                        self.assertEqual(trace['reads'], [
                            [r, column, base + r * stride + column, column * 32 + r, r * 32 + column]
                            for r in range(32)])
                        batches = service(trace['reads'])
                        self.assertEqual(len(batches), 32 if stride == 32 else 1)
                        self.assertEqual(sorted(lane for b in batches for lane in b), list(range(32)))

    def test_cross_warp_data_producer_matches_consumer(self):
        for column in range(32):
            trace = reference(33, column)
            for lane, col, word, oi, value in trace['reads']:
                producer_tx, producer_ty, producer_j = column, lane % 8, lane // 8 * 8
                self.assertEqual(value, (producer_ty + producer_j) * 32 + producer_tx)
                self.assertEqual(word, (producer_ty + producer_j) * 33 + producer_tx)
                self.assertEqual(oi, col * 32 + lane)

    def test_default_column_values_and_global_writes(self):
        trace = get_trace(32)
        self.assertEqual([r[4] for r in trace['reads'][:8]], [3, 35, 67, 99, 131, 163, 195, 227])
        self.assertEqual([r[3] for r in trace['reads']], list(range(96, 128)))
        self.assertEqual({r[2] % 32 for r in trace['reads']}, {3})

    def test_identical_address_broadcast(self):
        self.assertEqual(service([[r, 0, 7, r, 7] for r in range(32)]), [list(range(32))])
        reads = [[0, 0, 0, 0, 0], [1, 0, 0, 1, 0], [2, 0, 32, 2, 32], [3, 0, 32, 3, 32]]
        self.assertEqual(service(reads), [[0, 1], [2, 3]])
        self.assertEqual(service([]), [])

    def test_generated_payload_format_round_trip_not_gpu_execution(self):
        data = {'column': 3, 'runs': [reference(32, 3, 32), reference(33, 3, 96)]}
        assignment = 'GPU_TRACE = ' + json.dumps(data)
        local = {}
        exec(assignment, local)
        NS['GPU_TRACE'] = local['GPU_TRACE']
        self.assertEqual(get_trace(32), data['runs'][0])
        self.assertEqual(get_trace(33), data['runs'][1])

    def test_corrupt_trace_rejected(self):
        def fresh():
            return {'column': 3, 'runs': [reference(32, 3), reference(33, 3)]}
        for bad in ([], {}, {'column': 4, 'runs': []}, {'column': 3, 'runs': []}):
            NS['GPU_TRACE'] = bad
            with self.assertRaises(ValueError):
                get_trace(32)
        for key in ('stride', 'base_word', 'loads', 'reads', 'verified_output_words'):
            NS['GPU_TRACE'] = fresh()
            del NS['GPU_TRACE']['runs'][0][key]
            with self.assertRaises(ValueError):
                get_trace(32)
        NS['GPU_TRACE'] = fresh()
        NS['GPU_TRACE']['runs'][0]['reads'][7][4] += 1
        with self.assertRaises(ValueError):
            get_trace(32)

    def test_unsupported_configuration_and_duplicate_run_rejected(self):
        NS['GPU_TRACE'] = {'column': 3, 'runs': [reference(32, 3)] * 2}
        with self.assertRaises(ValueError):
            get_trace(32)
        NS['GPU_TRACE'] = {'column': 3, 'runs': [reference(32, 3)]}
        NS['GPU_TRACE']['runs'][0]['base_word'] = 1.5
        with self.assertRaises(ValueError):
            get_trace(32)


class LayoutTests(unittest.TestCase):
    def test_request_tray_spacing(self):
        points = [NS["queue_point"](i) for i in range(32)]
        assert_nonoverlap(self, points, NS["QUEUE_W"], NS["QUEUE_H"])
        assert_inside(self, points, NS["QUEUE_W"], NS["QUEUE_H"], (-6.835, -2.15, -0.185, -0.60))

    def test_bank_card_spacing(self):
        points = [NS["bank_center"](i) for i in range(32)]
        assert_nonoverlap(self, points, 0.65, 0.54)
        assert_inside(self, points, 0.65, 0.54, (0.105, -0.50, 6.755, 2.10))

    def test_register_cells_and_packets_fit(self):
        centers = [NS["register_point"](i) for i in range(32)]
        assert_nonoverlap(self, centers, NS["REGISTER_CELL_W"], NS["REGISTER_CELL_H"])
        assert_inside(self, centers, NS["REGISTER_CELL_W"], NS["REGISTER_CELL_H"], (0.105, -2.40, 6.755, -1.2))
        self.assertLess(NS["QUEUE_W"] * NS["REGISTER_SCALE"], NS["REGISTER_CELL_W"])
        self.assertLess(NS["QUEUE_H"] * NS["REGISTER_SCALE"], 0.20)

    def test_all_column_and_transposed_row_positions(self):
        for c in range(32):
            selected = [NS["tile_point"](r, c) for r in range(32)]
            output = [NS["output_point"](r, c) for r in range(32)]
            self.assertEqual({p[0] for p in selected}, {NS["tile_point"](0, c)[0]})
            self.assertEqual({p[1] for p in output}, {NS["tile_point"](c, 0)[1]})
            assert_nonoverlap(self, selected, 0.097, 0.097)
            assert_nonoverlap(self, output, 0.097, 0.097)
            assert_inside(self, selected + output, 0.097, 0.097, (1.55, -1.90, 5.07, 1.62))

    def test_read_requests_do_not_cross_code_area(self):
        # Point trajectories only, not a renderer. Verify the queue -> bank path
        # stays below the lowest source-line highlight while within the code pane.
        code_bottom = NS["CODE_TOP"] - (len(NS["code_entries"]("read")) - 1) * NS["CODE_STEP"] - NS["CODE_LINE_H"] / 2
        for base in (0, 1, 31):
            for stride in (32, 33):
                for column in range(32):
                    for lane, _, word, _, _ in reference(stride, column, base)["reads"]:
                        a, b = NS["queue_point"](lane), NS["bank_point"](word % 32)
                        # Linear geometric segment endpoints plus its crossing of x=-0.185.
                        points = [a]
                        x_cross = -0.185 + NS["QUEUE_W"] / 2
                        t = (x_cross - a[0]) / (b[0] - a[0])
                        if 0 <= t <= 1:
                            points.append((x_cross, a[1] + t * (b[1] - a[1]), 0))
                        self.assertLessEqual(max(p[1] for p in points) + NS["QUEUE_H"] / 2,
                                             code_bottom + 1e-8)


class SyntaxTests(unittest.TestCase):
    def test_real_token_ranges_are_nonoverlapping(self):
        for stride in (32, 33):
            for stage in ("load", "output", "read"):
                for _, line in NS["code_entries"](stage, stride):
                    spans = NS["syntax_spans"](line)
                    for a, b, tag in spans:
                        self.assertLess(a, b)
                        self.assertTrue(0 <= a < b <= len(line))
                    for left, right in zip(spans, spans[1:]):
                        self.assertLessEqual(left[1], right[0])

    def test_in_does_not_color_int_or_threadidx(self):
        line = "int x = threadIdx.x; tile[x][0] = in[x];"
        found = [(line[a:b], k) for a, b, k in NS["syntax_spans"](line)]
        self.assertEqual(found, [("int", "keyword"), ("threadIdx.x", "index"),
                                 ("tile", "tile"), ("0", "constant"), ("in", "in")])

    def test_comment_is_a_single_range(self):
        line = "// int tile in out 32 __syncthreads"
        self.assertEqual(NS["syntax_spans"](line), [(0, len(line), "comment")])

    def test_barrier_has_distinct_syntax_role(self):
        self.assertEqual(NS["syntax_spans"]("__syncthreads();"), [(0, 13, "barrier")])


class SourceContracts(unittest.TestCase):
    def test_palette_is_typed_once(self):
        palette = {"BG", "PANEL", "PANEL_LIGHT", "EDGE", "INK", "MUTED", "CYAN",
                   "GREEN", "AMBER", "VIOLET", "PINK", "CORAL"}
        seen = set()
        for node in TREE.body:
            if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name):
                name = node.targets[0].id
                if name in palette:
                    self.assertIsInstance(node.value, ast.Call)
                    self.assertEqual(ast.unparse(node.value.func), "color_from_hex")
                    seen.add(name)
        self.assertEqual(seen, palette)
        for call in (n for n in ast.walk(TREE) if isinstance(n, ast.Call)):
            for kw in call.keywords:
                if kw.arg == "color" and isinstance(kw.value, ast.Constant):
                    self.assertNotIsInstance(kw.value.value, str)

    def test_each_source_step_exists_on_the_correct_page(self):
        steps = NS["SOURCE_STEPS"]
        for page, names in (("load", ("allocate", "coordinates", "loop", "bounds", "copy", "barrier")),
                            ("output", ("output_coordinates", "output_bounds", "read", "store")),
                            ("read", ("read", "store"))):
            ids = {k for k, _ in NS["code_entries"](page)}
            for name in names:
                self.assertTrue(set(steps[name]) <= ids)
        self.assertEqual(steps["copy"], ("load_dst", "load_src"))
        self.assertEqual(steps["read"], ("read_src",))
        self.assertEqual(steps["store"], ("store_dst",))

    def test_data_motion_establishes_focus_first(self):
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        method = next(n for n in scene.body if isinstance(n, ast.FunctionDef) and n.name == "move_step")
        exprs = [n.value for n in method.body if isinstance(n, ast.Expr) and isinstance(n.value, ast.Call)]
        self.assertEqual([ast.unparse(n.func) for n in exprs], ["self.focus", "self.play_for"])
        self.assertEqual(ast.unparse(exprs[0].args[0]), "step")

    def test_no_per_frame_callback_or_secondary_engine(self):
        classes = [n for n in TREE.body if isinstance(n, ast.ClassDef)]
        self.assertEqual([(n.name, ast.unparse(n.bases[0])) for n in classes], [("CudaTransposeBanks", "Scene")])
        calls = {ast.unparse(n.func) for n in ast.walk(TREE) if isinstance(n, ast.Call)}
        for bad in ("add_updater", "always_redraw", "UpdateFromFunc", "requestAnimationFrame"):
            self.assertFalse(any(c.endswith(bad) for c in calls))
        self.assertFalse(any(isinstance(n, ast.While) for n in ast.walk(TREE)))

    def test_source_assignments_match_cuda_recorder(self):
        cuda = ROOT.joinpath("tools/cuda-transpose/record_cuda.cu").read_text(encoding="utf-8")
        for page in ("load", "output"):
            for key, line in NS["code_entries"](page):
                if key == "declaration":  # Recorder templates stride; upstream uses +1.
                    continue
                self.assertIn(line.strip(), cuda)

    def test_load_ready_count_updates_after_motion(self):
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        method = next(n for n in scene.body if isinstance(n, ast.FunctionDef) and n.name == "load_tile")
        loop = next(n for n in ast.walk(method) if isinstance(n, ast.For) and isinstance(n.target, ast.Tuple))
        calls = [n for n in ast.walk(loop) if isinstance(n, ast.Call)]
        motion = next(n for n in calls if ast.unparse(n.func) == "self.move_step"
                      and n.args and isinstance(n.args[0], ast.Constant) and n.args[0].value == "copy")
        count = next(n for n in calls if ast.unparse(n.func) == "self.status"
                     and "k + 1" in ast.unparse(n))
        self.assertGreater(count.lineno, motion.end_lineno)
        for pause in [n for n in calls if ast.unparse(n.func) == "self.pause"]:
            self.assertGreater(pause.lineno, count.lineno)

    def test_global_store_waits_for_all_read_batches(self):
        scene = next(n for n in TREE.body if isinstance(n, ast.ClassDef))
        method = next(n for n in scene.body if isinstance(n, ast.FunctionDef) and n.name == "bank_replay")
        loop = next(n for n in method.body if isinstance(n, ast.For) and ast.unparse(n.iter) == "batches")
        store = next(n for n in ast.walk(method) if isinstance(n, ast.Call)
                     and ast.unparse(n.func) == "self.move_step" and n.args
                     and isinstance(n.args[0], ast.Constant) and n.args[0].value == "store")
        self.assertGreater(store.lineno, loop.end_lineno)
        for n in ast.walk(loop):
            if isinstance(n, ast.Call) and ast.unparse(n.func) == "self.move_step":
                self.assertEqual(n.args[0].value, "read")

    def test_common_scale_indentation_and_matched_service_time(self):
        self.assertIn("max_w = max(max_w, padded.width)", SOURCE)
        self.assertIn("indent * advance * factor", SOURCE)
        self.assertIn("new.scale(self.code_scale)", SOURCE)
        self.assertEqual(SOURCE.count("seconds=ROUND_IN"), 1)
        self.assertEqual(SOURCE.count("seconds=ROUND_OUT"), 1)
        self.assertIn("same value", SOURCE.lower())
        self.assertIn("GPU-recorded selected read addresses + values", SOURCE)
        self.assertIn("not GPU cycles or a benchmark", SOURCE)


HELPERS = {'packet', 'packet_leaves', 'packet_motion'}
PURE_CONSTANTS = {'QUEUE_W', 'QUEUE_H', 'CODE_HIGHLIGHT_FILL', 'CODE_HIGHLIGHT_STROKE', 'SOURCE_STEPS'}


def helper_namespace():
    """Compile only helpers, numeric inputs and focus; never load a fake noon."""
    nodes = []
    for node in TREE.body:
        if isinstance(node, ast.FunctionDef) and node.name in HELPERS:
            nodes.append(node)
        if isinstance(node, ast.Assign):
            names = {n.id for target in node.targets for n in ast.walk(target)
                     if isinstance(n, ast.Name)}
            if names and names <= PURE_CONSTANTS:
                nodes.append(node)
        if isinstance(node, ast.ClassDef) and node.name == 'CudaTransposeBanks':
            nodes.extend(n for n in node.body if isinstance(n, ast.FunctionDef)
                         and n.name == 'focus')
    ns = {}
    exec(compile(ast.Module(body=nodes, type_ignores=[]), '<authoring helper unit tests>', 'exec'), ns)
    return ns


def leaf_spy(name):
    leaf = Mock(name=name, spec=['scale', 'animate', 'move_to'])
    builder = Mock(name=name + '.animate', spec=['move_to', 'scale', 'set_fill'])
    builder.move_to.return_value = builder
    builder.scale.return_value = builder
    builder.set_fill.return_value = builder
    leaf.animate = builder
    leaf.scale.return_value = leaf
    return leaf


def family_constructor_calls(tree):
    return [node for node in ast.walk(tree)
            if isinstance(node, ast.Call)
            and isinstance(node.func, ast.Name)
            and node.func.id in ('VGroup', 'Group')]


class PacketMotionTests(unittest.TestCase):
    def setUp(self):
        self.ns = helper_namespace()

    def test_no_family_constructors_in_demo(self):
        self.assertEqual(family_constructor_calls(TREE), [])

    def test_guard_detects_original_packet_pattern(self):
        # Negative control for the SOURCE guard, not a claim of Noon execution.
        old = ast.parse('def packet(lane, point):\n    return VGroup(rect(), label())\n')
        self.assertEqual(len(family_constructor_calls(old)), 1)

    def test_packet_is_two_centered_leaf_references(self):
        for lane in range(32):
            with self.subTest(lane=lane):
                body, number = leaf_spy('body'), leaf_spy('number')
                self.ns.update(rect=Mock(return_value=body), label=Mock(return_value=number),
                               ROW_COLORS=('cyan', 'amber', 'violet', 'pink'), BG=object())
                p = (-1.25, 2.1, 0)
                parts = self.ns['packet'](lane, p, scale=0.20)
                self.assertIsInstance(parts, tuple)
                self.assertIs(parts[0], body)
                self.assertIs(parts[1], number)
                self.ns['rect'].assert_called_once_with(
                    p[0], p[1], self.ns['QUEUE_W'], self.ns['QUEUE_H'],
                    self.ns['ROW_COLORS'][lane // 8], 1, 0)
                self.ns['label'].assert_called_once_with(
                    lane, p[0], p[1], 12, self.ns['BG'], width=self.ns['QUEUE_W'] - 0.06)
                body.scale.assert_called_once_with(0.20)
                number.scale.assert_called_once_with(0.20)

    def test_default_packet_does_not_rescale(self):
        parts = (leaf_spy('body'), leaf_spy('number'))
        self.ns.update(rect=Mock(return_value=parts[0]), label=Mock(return_value=parts[1]),
                       ROW_COLORS=(1, 2, 3, 4), BG=object())
        result = self.ns['packet'](0, (0, 0, 0))
        self.assertEqual(result, parts)
        for part in parts:
            part.scale.assert_not_called()

    def test_leaf_flattening_preserves_order_and_identity(self):
        packets = [(object(), object()) for _ in range(32)]
        leaves = self.ns['packet_leaves'](packets)
        self.assertEqual(len(leaves), 64)
        self.assertEqual(len({id(leaf) for leaf in leaves}), 64)
        for lane, (body, number) in enumerate(packets):
            self.assertIs(leaves[2 * lane], body)
            self.assertIs(leaves[2 * lane + 1], number)
        self.assertEqual(self.ns['packet_leaves']([]), [])

    def test_move_only_uses_both_leaf_animation_builders(self):
        parts = (leaf_spy('body'), leaf_spy('number'))
        dest = (1.1, -2.0, 0)
        animations = self.ns['packet_motion'](parts, dest)
        self.assertEqual(animations, [part.animate for part in parts])
        for part in parts:
            part.animate.move_to.assert_called_once_with(dest)
            part.animate.scale.assert_not_called()
            # Helpers must not move or scale the live source before play.
            part.move_to.assert_not_called()
            part.scale.assert_not_called()

    def test_all_motion_scales_apply_after_target_move(self):
        for factor in (5.0, 0.58, 0.20 / 0.58):
            with self.subTest(factor=factor):
                parts = (leaf_spy('body'), leaf_spy('number'))
                dest = (4.5, -1.2, 0)
                animations = self.ns['packet_motion'](parts, dest, scale=factor)
                self.assertEqual(animations, [p.animate for p in parts])
                for part in parts:
                    self.assertEqual(part.animate.method_calls,
                                     [call.move_to(dest), call.scale(factor)])
                    part.move_to.assert_not_called()
                    part.scale.assert_not_called()

    def test_padded_batch_stays_64_parallel_leaf_requests(self):
        packets = [(leaf_spy(f'body{i}'), leaf_spy(f'label{i}')) for i in range(32)]
        animations = [a for lane, parts in enumerate(packets)
                      for a in self.ns['packet_motion'](parts, (lane, 0, 0))]
        self.assertEqual(len(animations), 64)
        self.assertEqual(len({id(a) for a in animations}), 64)
        for lane, parts in enumerate(packets):
            for part in parts:
                part.animate.move_to.assert_called_once_with((lane, 0, 0))

    def test_four_motion_sites_all_use_leaf_helper(self):
        calls = [n for n in ast.walk(TREE) if isinstance(n, ast.Call)
                 and isinstance(n.func, ast.Name) and n.func.id == 'packet_motion']
        self.assertEqual(len(calls), 4)
        self.assertIn('self.add(*packet_leaves(requests))', SOURCE)
        self.assertIn('result + packet_leaves(requests) + self.code_objects', SOURCE)
        self.assertNotIn('requests[lane].animate', SOURCE)
        helpers = [n for n in TREE.body if isinstance(n, ast.FunctionDef) and n.name in HELPERS]
        forbidden = {'play', 'play_for', 'move_step', 'wait', 'pause', 'AnimationGroup', 'Succession'}
        for node in helpers:
            for c in (n for n in ast.walk(node) if isinstance(n, ast.Call)):
                name = c.func.id if isinstance(c.func, ast.Name) else getattr(c.func, 'attr', '')
                self.assertNotIn(name, forbidden)


class HighlightTests(unittest.TestCase):
    def setUp(self):
        self.ns = helper_namespace()

    def scene_spy(self, active=()):
        keys = {'load_dst', 'load_src', 'read_src', 'store_dst'}
        return SimpleNamespace(active_keys=active,
                               highlights={key: leaf_spy(key) for key in keys}, play_for=Mock())

    def test_active_highlight_is_seven_percent_fill_no_outline(self):
        self.assertEqual(self.ns['CODE_HIGHLIGHT_FILL'], 0.07)
        self.assertEqual(self.ns['CODE_HIGHLIGHT_STROKE'], 0)
        self.assertIn('CODE_HIGHLIGHT_COLOR = color_from_hex("#536678")', SOURCE)
        self.assertNotIn('.set_opacity(', SOURCE)
        s = self.scene_spy(('load_dst', 'load_src'))
        self.ns['focus'](s, 'read')
        for key in ('load_dst', 'load_src'):
            s.highlights[key].animate.set_fill.assert_called_once_with(opacity=0.0)
        s.highlights['read_src'].animate.set_fill.assert_called_once_with(opacity=0.07)
        self.assertEqual(s.active_keys, ('read_src',))
        self.assertEqual(len(s.play_for.call_args.args), 3)
        self.assertEqual(s.play_for.call_args.kwargs, {'seconds': 0.16})

    def test_focus_highlights_both_assignment_lines(self):
        s = self.scene_spy()
        self.ns['focus'](s, 'copy')
        self.assertEqual(s.active_keys, ('load_dst', 'load_src'))
        for key in s.active_keys:
            s.highlights[key].animate.set_fill.assert_called_once_with(opacity=0.07)
        self.assertEqual(len(s.play_for.call_args.args), 2)

    def test_repeated_focus_does_not_restart_animation(self):
        s = self.scene_spy(('read_src',))
        self.ns['focus'](s, 'read')
        s.play_for.assert_not_called()
        s.highlights['read_src'].animate.set_fill.assert_not_called()

    def test_missing_source_line_fails_before_play(self):
        s = self.scene_spy()
        del s.highlights['read_src']
        with self.assertRaisesRegex(ValueError, 'missing from the displayed CUDA page'):
            self.ns['focus'](s, 'read')
        s.play_for.assert_not_called()

class AttributionTests(unittest.TestCase):
    def test_short_header_credits_original_author_and_demo_purpose(self):
        header = ast.get_docstring(TREE)
        self.assertIn("reproduced with Noon for demonstration purposes", header)
        self.assertIn("Arash (ourarash)", header)
        self.assertIn("03_transpose_bank_conflicts_visualization.html", header)
        self.assertIn("03_transpose_padded.cu", header)
        self.assertIn("cuda_transpose_banks.LICENSE", header)
        self.assertLessEqual(len(header.splitlines()), 15)
        self.assertNotIn("Permission is hereby granted", header)
        self.assertNotIn('THE SOFTWARE IS PROVIDED', header)

    def test_mit_notice_is_preserved_beside_the_demo(self):
        notice = SCENE_PATH.with_suffix(".LICENSE").read_text(encoding="utf-8")
        for phrase in ("MIT License", "Copyright (c) 2026 Arash",
                       "Permission is hereby granted", "THE SOFTWARE IS PROVIDED"):
            self.assertIn(phrase, notice)


if __name__ == "__main__":
    unittest.main(verbosity=2)
