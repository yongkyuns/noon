"""Unit tests for fidelity predicates and capture integrity; NOT browser/GPU runs.

Synthetic observations/images test that the checker rejects broken evidence.
The actual production scene and renderer are exercised by the separate browser
runner; no fake `noon` module is installed by these tests.
"""
import ast
import copy
import importlib.util
from pathlib import Path
import unittest
import tempfile

ROOT = Path(__file__).resolve().parents[1]
def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
probe = load('cuda_visual_probe', 'tools/cuda-transpose/visual_probe.py')
capture = load('cuda_capture', 'tools/cuda-transpose/capture_cuda.py')
SCENE = (ROOT / 'web/python/examples/cuda_transpose_banks.py').read_text()


def trace(stride=33, column=3, base=0):
    return {'stride':stride, 'base_word':base, 'loads':[256]*4,
            'verified_output_words':1024, 'reads':probe.expected_reads(stride,column,base)}


def observations(expected):
    # Unit input data only; these are not rendered results.
    return [{'lane':e['lane'], 'body':{'id':2*e['lane'], 'center':e['center'][:],
             'width':e['width'], 'height':e['height'], 'alpha':1,
             'rgb':[c/255 for c in e['rgb']]},
             'label':{'id':2*e['lane']+1, 'text':str(e['lane']), 'center':e['center'][:],
                      'width':e['width']*0.5, 'height':e['height']*0.5}}
            for e in expected]


class FidelityPredicates(unittest.TestCase):
    def test_all_columns_layouts_and_physical_bases(self):
        for base in (0,1,31,32,128):
            for stride in (32,33):
                for column in range(32):
                    probe.validate_trace(trace(stride,column,base),column)
                    words = [row[2] for row in trace(stride,column,base)['reads']]
                    self.assertEqual(len(set(w%32 for w in words)),32 if stride==33 else 1)

    def test_independent_lane9_fixture(self):
        self.assertEqual(probe.expected_reads(32,3,0)[9],[9,3,291,105,291])
        self.assertEqual(probe.expected_reads(33,3,0)[9],[9,3,300,105,291])
        self.assertEqual(291%32,3); self.assertEqual(300%32,12)

    def test_corrupt_trace_and_false_base_rejected(self):
        for change in (lambda t:t['reads'][9].__setitem__(2,301),
                       lambda t:t['reads'][9].__setitem__(4,290),
                       lambda t:t.__setitem__('base_word',True),
                       lambda t:t.__setitem__('loads',[256]*3)):
            t=trace(); change(t)
            with self.assertRaises(probe.FidelityError): probe.validate_trace(t,3)

    def test_endpoint_predicate_detects_wrong_bank_label_hidden_and_identity(self):
        want=[probe.expected_packet(i,'bank',3,33,0) for i in range(32)]
        original=observations(want)
        ids=probe.validate_packets(original,want)
        changes=[lambda x:x[9]['body']['center'].__setitem__(0,6.2),
                 lambda x:x[9]['label']['center'].__setitem__(0,6.2),
                 lambda x:x[9]['label'].__setitem__('text','8'),
                 lambda x:x[9]['body'].__setitem__('alpha',0),
                 lambda x:x[9]['body'].__setitem__('id',9000)]
        for change in changes:
            bad=copy.deepcopy(original); change(bad)
            with self.assertRaises(probe.FidelityError): probe.validate_packets(bad,want,ids)

    def test_nan_positions_rejected(self):
        with self.assertRaises(probe.FidelityError): probe.close([float('nan'),0],[0,0])

    def test_highlight_uses_observed_alpha_and_text_not_authored_key(self):
        h={'read_src':{'alpha':0.20,'text':probe.SOURCE_TEXT['read_src']},
           'store_dst':{'alpha':0,'text':probe.SOURCE_TEXT['store_dst']}}
        for value in h.values():
            value.update(center=[0,1.04], width=6, height=0.32,
                         text_center=[0,1], text_width=5, text_height=0.15)
        probe.validate_highlights(h,['read_src'])
        h['store_dst']['alpha']=0.2
        with self.assertRaises(probe.FidelityError): probe.validate_highlights(h,['read_src'])
        h['store_dst']['alpha']=0; h['read_src']['text']='tile[threadIdx.y][threadIdx.x];'
        with self.assertRaises(probe.FidelityError): probe.validate_highlights(h,['read_src'])

    def test_original_writer_and_consumer_differ(self):
        for r in range(32):
            for c in range(32):
                writer=(c,r%8,r//8*8); consumer=(r,c%8,c//8*8)
                self.assertEqual((writer[1]+writer[2])*32+writer[0],r*32+c)
                self.assertEqual(consumer[0]*32+consumer[1]+consumer[2],r*32+c)

    def test_every_operation_is_observed_without_runtime_replacement(self):
        tree=ast.parse(SCENE)
        kinds=set()
        for node in ast.walk(tree):
            if isinstance(node,ast.Dict):
                for k,v in zip(node.keys,node.values):
                    if isinstance(k,ast.Constant) and k.value=='kind' and isinstance(v,ast.Constant): kinds.add(v.value)
        self.assertEqual(kinds,{'load','tray','bank','register','store'})
        self.assertIn('self.play(*animations, run_time=seconds / SPEED)',SCENE)
        self.assertIn('self.wait(seconds / SPEED)',SCENE)
        self.assertNotIn('add_updater(',SCENE)
        code=(ROOT/'tools/cuda-transpose/visual_probe.py').read_text()
        self.assertNotIn('scene.play(',code)
        self.assertNotIn('scene.wait(',code)
        self.assertNotIn('reference_trace(',code)
        self.assertNotIn('bank_point(',code)


def source_page(kind="load"):
    keys = {"load": ["declaration", "input_x", "input_y", "load_loop", "load_bounds", "load_dst", "load_src", "close_if", "close_loop", "sync"],
            "output": ["output_x", "output_y", "store_loop", "store_bounds", "store_dst", "read_src", "close_if", "close_loop"],
            "read": ["store_dst", "read_src"]}[kind]
    rows, numbers, regions = {}, [], {}
    for index, key in enumerate(keys):
        y = 2.0 - 0.37 * index
        text = probe.SOURCE_TEXT.get(key, "__shared__ float tile[TILE_DIM][TILE_DIM];" if key == "declaration" else "statement;")
        rows[key] = {"id": index + 1, "center": [-3.5, y], "width": 5.7, "height": 0.15, "text": text}
        numbers.append({"id": index + 101, "center": [-6.52, y], "width": 0.12,
                        "height": 0.10, "text": "%02d" % (index + 1)})
        regions[key] = {"id": index + 201, "text_id": index + 1,
                        "center": [-3.49, y + 0.02], "width": 6.4, "height": 0.32, "stroke_width": 0}
    return {"page": kind, "rows": rows, "numbers": numbers, "regions": regions,
            "source": "\n".join(row["text"] for row in rows.values()),
            "source_font_size": 16, "gutter_font_size": 11,
            "panel": {"id": 900, "center": [-3.49, 0.3], "width": 6.4, "height": 3.7, "alpha": 0}}


class SourcePagePredicates(unittest.TestCase):
    def test_allocated_rows_cover_ink_without_matching_ink_centers(self):
        for kind in ("load", "output", "read"):
            page = source_page(kind)
            probe.validate_source_page(page)
        page = source_page()
        page["regions"]["load_src"]["center"][1] += 0.2
        with self.assertRaisesRegex(probe.FidelityError, "highlight-position"):
            probe.validate_source_page(page)

    def test_unchanged_source_and_gutter_identity_survives_bounded_edit(self):
        previous = source_page()
        current = copy.deepcopy(previous)
        current["rows"]["declaration"]["text"] = current["rows"]["declaration"]["text"].replace(
            "[TILE_DIM][TILE_DIM];", "[TILE_DIM][TILE_DIM + 1];")
        current["rows"]["declaration"]["id"] = 301
        current["regions"]["declaration"]["text_id"] = 301
        current["source"] = "\n".join(row["text"] for row in current["rows"].values())
        probe.validate_source_edit(previous, current)
        for mutate in (lambda p: p["rows"]["load_src"]["center"].__setitem__(0, -3.4),
                       lambda p: p["numbers"][0].__setitem__("id", 1001),
                       lambda p: p["regions"]["declaration"]["center"].__setitem__(1, 2.2),
                       lambda p: p["rows"]["declaration"]["center"].__setitem__(1, 2.5)):
            bad = copy.deepcopy(current); mutate(bad)
            with self.assertRaisesRegex(probe.FidelityError, "source-edit"):
                probe.validate_source_edit(previous, bad)

    def test_zero_padding_and_smaller_numbers_are_observed(self):
        for mutate in (lambda p: p["numbers"][0].__setitem__("text", "1"),
                       lambda p: p.__setitem__("gutter_font_size", 16),
                       lambda p: p["numbers"][0].__setitem__("height", 0.2)):
            bad = source_page(); mutate(bad)
            with self.assertRaisesRegex(probe.FidelityError, "gutter"):
                probe.validate_source_page(bad)

    def test_compact_page_clearance_uses_all_corridors_and_actual_row_allocation(self):
        page = source_page("read")
        probe.validate_compact_clearance(page["regions"])
        for region in page["regions"].values():
            region["center"][1] -= 1
        with self.assertRaisesRegex(probe.FidelityError, "packet-clearance"):
            probe.validate_compact_clearance(page["regions"])


class CapturePredicates(unittest.TestCase):
    def full_run(self,stride=33,column=3):
        run=trace(stride,column,64)
        run['load_accesses']=[[r,c,64+r*stride+c,r*32+c,r*32+c] for r in range(32) for c in range(32)]
        run['read_accesses']=[[r,c,64+r*stride+c,c*32+r,r*32+c] for c in range(32) for r in range(32)]
        run['output']=[(i%32)*32+i//32 for i in range(1024)]
        run['original_output']=run['output'][:]
        return run

    def test_full_coverage_required_not_just_selected_row(self):
        for stride in (32,33):
            r=self.full_run(stride); capture.validate_run(r,3)
            r['load_accesses'][100][4]+=1
            with self.assertRaisesRegex(ValueError,'load'): capture.validate_run(r,3)

    def test_original_output_mismatch_is_fatal(self):
        r=self.full_run(); r['original_output'][512]+=1
        with self.assertRaisesRegex(ValueError,'original'): capture.validate_run(r,3)

    def test_missing_records_not_replaced_with_reference(self):
        r=self.full_run(); del r['read_accesses']
        with self.assertRaisesRegex(ValueError,'shared read'): capture.validate_run(r,3)

    def test_cpu_payload_cannot_claim_gpu_validation(self):
        with self.assertRaisesRegex(ValueError,'not a CUDA capture'):
            capture.validate_bundle({'schema':1,'provenance':{'origin':'cpu-reference'}})

    def test_source_fingerprints_cover_both_actual_cuda_implementations(self):
        hashes=capture.source_fingerprints()
        self.assertEqual(set(hashes),set(capture.CUDA_FILES))
        self.assertTrue(all(len(v)==64 for v in hashes.values()))
        src=(ROOT/'tools/cuda-transpose/record_cuda.cu').read_text()
        self.assertIn('transpose_shared<<<',src); self.assertIn('transpose_padded<<<',src)
        self.assertIn('result.original_output != result.output',src)

    def test_required_sanitizers_are_not_best_effort(self):
        src=(ROOT/'tools/cuda-transpose/capture_cuda.py').read_text()
        self.assertIn('"--error-exitcode", "1"',src)
        self.assertIn('"memcheck", "racecheck", "synccheck"',src)
        self.assertIn('capture is stale',src)


class BrowserRunnerContracts(unittest.TestCase):
    def test_real_renderer_and_authored_time_sampling_are_required(self):
        s=(ROOT/'scripts/cuda-transpose-qualification.mjs').read_text()
        for term in ('PythonAuthoringClient','AuthoringExecutionClient',"pacing:'external_samples'",
                     'sampleToAuthoredTime',"locator('#scene').screenshot",'execution.debugFrame()',
                     'CUDA_PIXELS','expected-rejection'):
            self.assertIn(term,s)
        self.assertNotIn('waitForTimeout(',s)
        self.assertIn('async def _qualified_cuda_construct(self)',s)
        self.assertIn('await _original_cuda_construct(self)',s)
        self.assertIn('CudaTransposeBanks.construct = _qualified_cuda_construct',s)
        self.assertNotIn('result.construct()',s)
        for mutation in ('wrong_bank','wrong_highlight','detached_label','invisible_packet','early_store','dim_highlight'):
            self.assertIn(mutation,s)

    def test_missing_gpu_evidence_cannot_pass_strict_mode(self):
        s=(ROOT/'scripts/cuda-transpose-qualification.mjs').read_text()
        self.assertIn('NOON_CUDA_REQUIRE_GPU',s)
        self.assertIn('a CPU fixture is not GPU evidence',s)
        self.assertIn('cudaValidated: false',s)


class ModelComparisonTests(unittest.TestCase):
    """Synthetic capture rows test the comparator, not CUDA execution."""

    def bundle(self):
        # Physical bases deliberately rotate the bank numbering between runs.
        return {"columns": {str(c): {"column": c, "runs": [
            trace(stride, c, (0, 1, 31, 32, 128)[(c + stride) % 5])
            for stride in (32, 33)]} for c in range(32)}}

    def test_actual_demo_model_matches_all_columns_without_injecting_capture(self):
        bundle = self.bundle()
        before = copy.deepcopy(bundle)
        result = capture.compare_demo_model(bundle)
        self.assertEqual(result["cases"], 64)
        self.assertEqual(result["columns"], 32)
        self.assertEqual(result["layouts"], [32, 33])
        self.assertEqual(result["addressBasis"], "tile-relative")
        self.assertEqual(result["sceneSha256"], capture.digest(
            (ROOT / "web/python/examples/cuda_transpose_banks.py").read_bytes()))
        self.assertEqual(bundle, before)

    def test_captured_wrong_word_value_output_or_base_is_rejected(self):
        for change in (lambda r: r["reads"][9].__setitem__(2, r["reads"][9][2] + 1),
                       lambda r: r["reads"][9].__setitem__(3, 999),
                       lambda r: r["reads"][9].__setitem__(4, -1),
                       lambda r: r.__setitem__("base_word", r["base_word"] + 1),
                       lambda r: r.__setitem__("loads", [255] * 4)):
            bundle = self.bundle()
            change(bundle["columns"]["31"]["runs"][1])
            with self.assertRaisesRegex(ValueError, "demo model differs"):
                capture.compare_demo_model(bundle)

    def test_missing_layout_cannot_pass_comparison(self):
        bundle = self.bundle()
        bundle["columns"]["31"]["runs"].pop()
        with self.assertRaisesRegex(ValueError, "both layouts"):
            capture.compare_demo_model(bundle)

    def check_source(self, source):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "scene.py"
            path.write_text(source, encoding="utf-8")
            return capture.compare_demo_model(self.bundle(), path)

    def test_bug_in_real_demo_model_is_detected(self):
        old = "selected.append([tx, c, word, c * 32 + tx, tile[word]])"
        self.assertEqual(SCENE.count(old), 1)
        bad = SCENE.replace(old, "selected.append([tx, c, word, c * 32 + tx, tile[word] + 1])")
        with self.assertRaisesRegex(ValueError, "demo model differs"):
            self.check_source(bad)

    def test_wrong_service_multiplicity_is_detected(self):
        tree = ast.parse(SCENE)
        fn = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "service_batches")
        lines = SCENE.splitlines(keepends=True)
        bad = "".join(lines[:fn.lineno-1]) + "def service_batches(reads):\n    return [list(range(32))]\n" + "".join(lines[fn.end_lineno:])
        with self.assertRaisesRegex(ValueError, "service rounds differ"):
            self.check_source(bad)

    def test_comparison_does_not_import_or_execute_the_scene(self):
        source = "raise AssertionError('the scene must not execute in this comparator')\n" + SCENE
        self.assertEqual(self.check_source(source)["cases"], 64)

    def test_missing_model_helper_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "helpers missing"):
            self.check_source(SCENE.replace("def reference_trace(", "def renamed_model("))

    def test_runner_preserves_demo_model_and_binds_comparison_to_tested_source(self):
        runner = (ROOT / "scripts/cuda-transpose-qualification.mjs").read_text()
        self.assertNotIn("GPU_TRACE", runner)
        self.assertNotIn("capture.columns", runner)
        self.assertIn("verification.modelComparison.sceneSha256, report.sceneSha256", runner)
        self.assertIn("verification.modelComparison.cases, 64", runner)
        self.assertLess(runner.index("report.status='passed';"),
                        runner.index("report.cudaValidated = capture !== null;"))

    def test_recorder_emits_full_json_not_demo_playback_assignment(self):
        recorder = (ROOT / "tools/cuda-transpose/record_cuda.cu").read_text()
        self.assertNotIn("GPU_TRACE", recorder)
        self.assertNotIn("bool full", recorder)
        self.assertNotIn("if (full)", recorder)
        for field in ("load_accesses", "read_accesses", "original_output"):
            self.assertIn(field, recorder)


if __name__=='__main__':
    unittest.main(verbosity=2)
