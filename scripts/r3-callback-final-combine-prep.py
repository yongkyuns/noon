"""Development-only #1350 combination with independently landed #1351."""
import ast
import os
import re
import subprocess as sp
import tempfile
from pathlib import Path

CANDIDATE = os.environ['CANDIDATE']
BASE = os.environ['BASE']
PREVIOUS_BASE = 'b9d6c4cbd82a6333551663358946294538cc3842'

def git(*args):
    return sp.check_output(['git', *args], text=True)

def source(ref, path):
    return git('show', f'{ref}:{path}')

assert git('rev-parse', 'HEAD').strip() == CANDIDATE
assert sp.run(['git', 'merge-base', '--is-ancestor', PREVIOUS_BASE, BASE]).returncode == 0
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
overlap = {'web/python/_manim_updaters.py', 'web/python/test_updater_snapshot.py'}
landed_delta = set(git('diff', '--name-only', PREVIOUS_BASE, BASE).splitlines())
assert landed_delta.intersection(expected) == overlap
# Only the already-reviewed master opacity methods/tests overlap. Require Git's
# ordinary clean merge; do not resolve an unexpected conflict as ours/theirs.
sp.run(['git', 'merge', '--no-commit', '--no-ff', BASE], check=True)
assert not git('diff', '--name-only', '--diff-filter=U').strip()
for path in expected - overlap:
    assert Path(path).read_text() == source(CANDIDATE, path), path
# Remove only the known old #1350 patch from private copies of the two merged
# files. The remainder must be byte-for-byte the new master's complete content.
# This independently protects #1351 and every previously landed test/body.
patch = git('diff', PREVIOUS_BASE, CANDIDATE, '--', *sorted(overlap))
with tempfile.TemporaryDirectory() as temp:
    for path in overlap:
        target = Path(temp, path)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(Path(path).read_bytes())
    sp.run(['git', 'apply', '--reverse'], input=patch, text=True, cwd=temp, check=True)
    for path in overlap:
        assert Path(temp, path).read_text() == source(BASE, path), path
for path in landed_delta - expected:
    assert Path(path).read_text() == source(BASE, path), path
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
assert updaters.count('def paint_set_opacity(') == 1
assert 'test_vmobject_callback_opacity_uses_shared_paint_and_preserves_composite' in Path('web/python/test_updater_snapshot.py').read_text()
player = Path('crates/noon-web/src/semantic_execution_player.rs').read_text()
assert player.count('.required_callback_family_read(') == 1
assert re.search(r'#\[cfg\(any\(target_arch = "wasm32", test\)\)\]\s+pub fn required_callback_read_json', player)
assert 'impl From<CallbackReadRequestWire>' not in player
mapper = Path('crates/noon-web/src/authoring_error.rs').read_text()
assert 'impl From<noon_runtime::EvaluationError>' not in mapper
assert mapper.count('impl From<ExecutionSessionCallbackError>') == 1
sp.run(['cargo', 'fmt', '--all', '--', '--check'], check=True)
assert set(git('diff', '--cached', '--name-only', BASE).splitlines()) == expected
sp.run(['git', 'diff', '--cached', '--check'], check=True)
print('Preserved every #1350 error change and every landed #1351 opacity change; ten general and seven callback WASM tests retained.')
