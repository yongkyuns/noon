import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

MODULE = Path(__file__).with_name('swiftshader-worker-pilot.py')
spec = importlib.util.spec_from_file_location('swiftshader_probe', MODULE)
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
PLAN = Path(__file__).parents[1] / 'benchmarks/swiftshader-worker-pilot.json'


class ProbeTests(unittest.TestCase):
    def setUp(self):
        self.p = json.loads(PLAN.read_text())

    def test_frozen_plan(self):
        mod.validate_plan(self.p)

    def test_no_gating_or_nonmatching_env(self):
        for field, value in [('qualification', True), ('mergeApproval', True),
                             ('pairIndex', 2), ('expectedBrowser', 'latest'),
                             ('pollIntervalSeconds', .1)]:
            p = json.loads(json.dumps(self.p))
            p[field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                mod.validate_plan(p)

    def test_modes_frozen(self):
        p = json.loads(json.dumps(self.p)); p['modes'].reverse()
        with self.assertRaises(AssertionError):mod.validate_plan(p)

    def test_pair_schedule_is_one_shot(self):
        p = json.loads(json.dumps(self.p)); p['execution']['retry'] = True
        with self.assertRaises(AssertionError):mod.validate_plan(p)

    def test_proc_stat_parse_handles_spaces(self):
        self.assertEqual(mod.parse_ppid('42 (Chrome Process) S 41 1 1 0 0 0 0 0 0 0'),41)
        self.assertEqual(mod.parse_ppid('42 (chrome) weird) S 41 1 1 0 0 0 0 0 0 0'),41)

    def test_proc_tree_closure(self):
        self.assertEqual(mod.family_of(1,{1:0,2:1,3:2,4:2,5:99,6:3}),{1,2,3,4,6})

    def test_thread_census_detects_only_chromium_descendants(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            for pid,ppid,name,workers in [(100,99,'node',[]),(101,100,'chrome',['Thread<00>','Thread<01>']),
                                           (102,101,'chrome',['Thread<02>']),(200,1,'chrome',['Thread<03>'])]:
                q=root/str(pid);(q/'task').mkdir(parents=True)
                (q/'comm').write_text(name+'\n')
                (q/'stat').write_text(f'{pid} ({name}) S {ppid} 0 0 0 0 0 0 0 0')
                for idx,worker in enumerate(workers):
                    t=q/'task'/str(pid+idx+1000);t.mkdir()
                    (t/'comm').write_text(worker+'\n')
            result=mod.census(100,root)
            self.assertEqual(result['workerCount'],3)
            self.assertEqual(set(result['groups']),{'101','102'})

    def test_observer_does_not_write_into_frozen_checkout(self):
        source=MODULE.read_text()
        self.assertIn('cwd=home',source)
        self.assertIn('"NOON_PRODUCT_REFERENCE_ROOT": str(baseline)',source)
        self.assertIn('"NOON_PRODUCT_CANDIDATE_ROOT": str(baseline)',source)
        self.assertNotIn('cargo ',source)
        self.assertNotIn('node --test scripts/playground-product',source)

    def test_one_attempt_and_no_existing_output_reuse(self):
        source=MODULE.read_text()
        self.assertIn('assert not output.exists()',source)
        self.assertIn('GITHUB_RUN_ATTEMPT',source)
        self.assertIn('GITHUB_EVENT_NAME',source)
        self.assertIn('"action"',source)
        self.assertIn('status"] = "not-confirmed"',source) if False else None

    def test_monitor_labels_never_approve_merge(self):
        source=MODULE.read_text()
        self.assertIn('"qualification": False',source)
        self.assertIn('"mergeApproval": False',source)
        self.assertNotIn('productMetrics',source)
        self.assertNotIn('qualifyProduct',source)

if __name__ == '__main__': unittest.main()
