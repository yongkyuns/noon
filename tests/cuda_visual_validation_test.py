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
            value.update(center=[0,1], width=6, text_center=[0,1], text_width=5)
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


if __name__=='__main__':
    unittest.main(verbosity=2)
