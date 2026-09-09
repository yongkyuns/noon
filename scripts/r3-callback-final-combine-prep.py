"""Development-only #1350 final combination; no new implementation or tests."""
import ast
import os
import re
import subprocess as sp
from pathlib import Path

CANDIDATE = os.environ['CANDIDATE']
BASE = os.environ['BASE']

def git(*args):
    return sp.check_output(['git', *args], text=True)

def source(ref, path):
    return git('show', f'{ref}:{path}')

assert git('rev-parse', 'HEAD').strip() == CANDIDATE
assert sp.run(['git', 'merge-base', '--is-ancestor',
               '406944f8a58218629c14ff247894b3fd2e8c471a', BASE]).returncode == 0
# This pinned master adds docs and the independently owned animation exports.
# Callback/runner changes must already be in the qualified Family candidate.
expected = {
    'crates/noon-web/src/authoring_error.rs',
    'crates/noon-web/src/semantic_execution_player.rs',
    'crates/noon-web/src/semantic_execution_player/callback_error_tests.rs',
    'web/python/_manim_updaters.py',
    'web/python/test_updater_snapshot.py',
    'web/python/test_noon_callback_errors_wasm.py',
    'scripts/typed-authoring-errors-smoke.mjs',
    'web/python/examples/ordinary_callback_sparse_reads.py',
}
landed_delta = set(git('diff', '--name-only',
    '406944f8a58218629c14ff247894b3fd2e8c471a', BASE).splitlines())
assert not landed_delta.intersection(expected), landed_delta.intersection(expected)
sp.run(['git', 'merge', '--no-commit', '--no-ff', BASE], check=True)
assert not git('diff', '--name-only', '--diff-filter=U').strip()
for path in expected:
    assert Path(path).read_text() == source(CANDIDATE, path), path
# Preserve every independently added test without adjusting counts or skips.
path = 'web/python/test_noon_errors_wasm.py'
assert Path(path).read_text() == source(BASE, path)
count = sum(isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef))
            and n.name.startswith('test_')
            for n in ast.walk(ast.parse(Path(path).read_text())))
assert count == 10, count
runner = Path('scripts/typed-authoring-errors-smoke.mjs').read_text()
assert f'assert.equal(report.python.additionalTests, {count});' in runner
assert 'assert.equal(report.python.callbackTests, 7);' in runner
updaters = Path('web/python/_manim_updaters.py').read_text()
assert 'expected_kind = "scalar" if kind == "scalar_signal" else kind' in updaters
player = Path('crates/noon-web/src/semantic_execution_player.rs').read_text()
assert player.count('.required_callback_family_read(') == 1
assert re.search(r'#\[cfg\(any\(target_arch = "wasm32", test\)\)\]\s+pub fn required_callback_read_json', player)
assert 'impl From<CallbackReadRequestWire>' not in player
# #1349 is separately owned and not in this pinned master. Its nested causes
# remain explicit inventory; no unlanded code is copied into this candidate.
mapper = Path('crates/noon-web/src/authoring_error.rs').read_text()
assert 'impl From<noon_runtime::EvaluationError>' not in mapper
assert mapper.count('impl From<ExecutionSessionCallbackError>') == 1
sp.run(['cargo', 'fmt', '--all', '--', '--check'], check=True)
assert set(git('diff', '--cached', '--name-only', BASE).splitlines()) == expected
sp.run(['git', 'diff', '--cached', '--check'], check=True)
print('Preserved all eight Family candidate files, all master changes, ten general and seven callback WASM tests.')
