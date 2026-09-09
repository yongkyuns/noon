"""Development-only correction of two Clippy findings in the new native test."""
from pathlib import Path
import os
import subprocess

base = os.environ['BASE']
candidate = os.environ['CANDIDATE']
assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip() == candidate
path = Path('crates/noon-web/src/semantic_execution_player/callback_error_tests.rs')
text = path.read_text()
for old, new in [
    ('.advance_to_callback_barrier(time)\n            .err()\n            .expect("the shared callback operation rejects non-finite time")',
     '.advance_to_callback_barrier(time)\n            .expect_err("the shared callback operation rejects non-finite time")'),
    ('.advance_segment_to_callback_barrier(segment, 0.125)\n        .err()\n        .expect("the shared segment operation rejects a pending callback")',
     '.advance_segment_to_callback_barrier(segment, 0.125)\n        .expect_err("the shared segment operation rejects a pending callback")'),
]:
    assert text.count(old) == 1, old
    text = text.replace(old, new, 1)
path.write_text(text)
subprocess.run(['cargo', 'fmt', '--all'], check=True)
assert subprocess.check_output(['git', 'diff', '--name-only'], text=True).splitlines() == [str(path)]
subprocess.run(['git', 'add', '--', str(path)], check=True)
expected = {
    'crates/noon-web/src/authoring_error.rs',
    'crates/noon-web/src/semantic_execution_player.rs',
    str(path), 'scripts/typed-authoring-errors-smoke.mjs',
    'web/python/_manim_updaters.py', 'web/python/test_updater_snapshot.py',
    'web/python/test_noon_callback_errors_wasm.py',
    'web/python/examples/ordinary_callback_sparse_reads.py',
}
assert set(subprocess.check_output(['git', 'diff', '--cached', '--name-only', base], text=True).splitlines()) == expected
subprocess.run(['git', 'merge-base', '--is-ancestor', base, candidate], check=True)
print('Only two err().expect() test expressions corrected; all production source unchanged.')
