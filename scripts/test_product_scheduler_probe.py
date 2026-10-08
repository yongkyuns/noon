import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("probe", Path(__file__).with_name("product-scheduler-probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)

class ProbeTests(unittest.TestCase):
    def test_parse_names_parentheses(self):
        fields = ["R"] + [str(i) for i in range(1, 50)]
        stat = probe.parse_stat("123 (Chrome (GPU) worker) " + " ".join(fields))
        self.assertEqual(stat['name'], 'Chrome (GPU) worker')
        self.assertEqual(stat['start_ticks'], 19)
        self.assertEqual(stat['last_cpu'], 36)

    def test_truncated_rejected(self):
        with self.assertRaises(ValueError):
            probe.parse_stat("123 (node) S 1 2")

    def test_descendants_and_pid_reuse(self):
        table = {10: {'ppid': 1, 'start_ticks': 50}, 11: {'ppid': 10, 'start_ticks': 51},
                 12: {'ppid': 11, 'start_ticks': 52}, 20: {'ppid': 1, 'start_ticks': 99},
                 21: {'ppid': 1, 'start_ticks': 60}}
        self.assertEqual(probe.descendants(table, 10, {(20, 1), (21, 60)}), {10, 11, 12, 21})

    def run_child(self, source, deadline=15):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'samples.jsonl'
            code = probe.observe([sys.executable, '-c', source], path, 0.05, deadline)
            lines = [json.loads(x) for x in path.read_text().splitlines()]
            self.assertEqual(lines[0]['qualification'], False)
            self.assertEqual(lines[-1]['kind'], 'end')
            self.assertGreaterEqual(lines[-1]['samples'], 1)
            self.assertTrue(all(x['scan_wall_ns'] >= x['observer_cpu_ns'] * 0.5
                                for x in lines if x['kind'] == 'sample'))
            return code, lines

    def test_real_child_tree_cpu_and_exit(self):
        code, lines = self.run_child('import subprocess,sys,time; p=subprocess.Popen([sys.executable,"-c","import time; t=time.monotonic()+.25\\nwhile time.monotonic()<t: pass"]); p.wait();time.sleep(.1)')
        self.assertEqual(code, 0)
        self.assertTrue(lines[-1]['active_schedstats'])
        self.assertTrue(any(len(set(t['tgid'] for t in x['tasks'])) >= 2
                            for x in lines if x['kind'] == 'sample'))

    def test_nonzero_is_not_hidden(self):
        code, _ = self.run_child('import time; time.sleep(.1); raise SystemExit(7)')
        self.assertEqual(code, 7)

    def test_watchdog_preserves_and_stops(self):
        code, lines = self.run_child('import time; time.sleep(30)', deadline=.15)
        self.assertEqual(code, 124)
        self.assertTrue(lines[-1]['timed_out'])

    def test_no_overwrite(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)/'old'; out.write_text('original')
            with self.assertRaises(FileExistsError):
                probe.observe([sys.executable, '-c', 'pass'], out, .1, 1)
            self.assertEqual(out.read_text(), 'original')

    def test_invalid_budget(self):
        with self.assertRaises(ValueError):
            probe.observe(['true'], Path('/not-created'), 0, 1)

    def test_missing_child_still_has_error_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)/'error.jsonl'
            with self.assertRaises(FileNotFoundError):
                probe.observe(['/no-such-probe-child'], out, .1, 1)
            kinds = [json.loads(x)['kind'] for x in out.read_text().splitlines()]
            self.assertEqual(kinds, ['start','error','end'])

if __name__ == '__main__':
    unittest.main()
