"""Development-only #1350/#1349 merge; no replacement test or error framework."""
from pathlib import Path
import ast
import os
import re
import subprocess

BASE = os.environ['BASE']
CANDIDATE = os.environ['CANDIDATE']

def git(*args):
    return subprocess.check_output(['git', *args], text=True)

def once(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new, 1)

assert git('rev-parse', 'HEAD').strip() == CANDIDATE
result = subprocess.run(['git', 'merge', '--no-commit', '--no-ff', BASE])
assert result.returncode in (0, 1)
runner = 'scripts/typed-authoring-errors-smoke.mjs'
unmerged = set(git('diff', '--name-only', '--diff-filter=U').splitlines())
assert unmerged == {runner}, unmerged

def resolve(match):
    ours, theirs = match.group(1), match.group(2)
    if ours.strip().startswith('report.python ='):
        assert 'report.advancement = ' in theirs
        return once(theirs, '{modules, tests, pyodideUrl}', '{modules, tests, callbackTests, pyodideUrl}')
    if ours.strip().startswith('json.dumps('):
        assert '"advancement": advancement_results' in theirs
        assert '"callbackTests": callback_result.testsRun' in ours
        return once(ours, '"additionalTests": result.testsRun', '"advancement": advancement_results, "additionalTests": result.testsRun')
    assert 'report.python.additionalTests, 10' in ours and 'report.python.additionalTests, 11' in theirs
    return theirs + '\n' + '\n'.join(line for line in ours.splitlines() if 'additionalTests' not in line) + '\n'

path = Path(runner)
text, count = re.subn(r'^<<<<<<<[^\n]*\n(.*?)^=======\n(.*?)^>>>>>>>[^\n]*\n', resolve, path.read_text(), flags=re.S | re.M)
assert count == 3, count
path.write_text(text)

path = Path('crates/noon-web/src/authoring_error.rs')
text = once(path.read_text(), '''E::Evaluation(cause) => Self::caused_by(
                "callback.evaluation",
                message,
                Self::unclassified("runtime.evaluation", &cause),
            ),''', '''E::Evaluation(cause) => {
                Self::caused_by("callback.evaluation", message, cause.into())
            }''')
text = once(text, '''E::Callback(cause) => Self::caused_by(
                "advance.callback",
                message,
                Self::unclassified("callback.unclassified", &cause),
            ),''', '''E::Callback(cause) => Self::caused_by("advance.callback", message, cause.into()),''')
assert text.count('impl From<ExecutionSessionCallbackError> for AuthoringFailure') == 1
assert text.count('impl From<noon_runtime::EvaluationError> for AuthoringFailure') == 1
path.write_text(text)

path = Path('crates/noon-web/src/semantic_execution_player/callback_error_tests.rs')
text = path.read_text()
assert 'callback_and_advancement_share_settled_nested_error_projection' not in text
text += '''
#[test]
fn callback_and_advancement_share_settled_nested_error_projection() {
    let (mut player, _) = player();
    let before = state(&player);
    for time in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = player
            .session
            .advance_to_callback_barrier(time)
            .err()
            .expect("the shared callback operation rejects non-finite time");
        // Exercise the existing shared wrapper conversion as well as the
        // actual callback producer. Segment admission rejects NaN earlier.
        let failure = AuthoringFailure::from(noon::ExecutionSegmentAdvanceError::from(error));
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "advance.callback");
        let callback = failure.cause.as_ref().unwrap();
        assert_eq!(callback.category, "invalid_input");
        assert_eq!(callback.code, "callback.evaluation");
        let cause = callback.cause.as_ref().unwrap();
        assert_eq!(cause.category, "invalid_input");
        assert_eq!(cause.code, "evaluation.invalid_time");
        assert!(cause.cause.is_none());
        assert!(failure.source().is_some());
        assert_eq!(state(&player), before);
    }

    let segment = player.session.wait_segment(0.25).unwrap();
    let phase = phase(&mut player);
    player.initial_delta_json().unwrap();
    let before = state(&player);
    let publication = player.session.publication_context();
    let error = player
        .session
        .advance_segment_to_callback_barrier(segment, 0.125)
        .err()
        .expect("the shared segment operation rejects a pending callback");
    let failure = AuthoringFailure::from(error);
    // Pending advancement remains outside the settled transaction categories;
    // preserve that explicit inventory instead of guessing from its message.
    assert_eq!(failure.category, "unclassified");
    assert_eq!(failure.code, "advance.callback");
    let cause = failure.cause.as_ref().unwrap();
    assert_eq!(cause.category, "unclassified");
    assert_eq!(cause.code, "callback.unclassified");
    assert!(cause.cause.is_none());
    assert!(failure.source().is_some());
    assert_eq!(state(&player), before);
    assert_eq!(player.session.publication_context(), publication);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    finish(&mut player, Some(&phase));
}
'''
path.write_text(text)

# Keep every independently added Python test and every native regression.
def test_names(text):
    return {node.name for node in ast.walk(ast.parse(text)) if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name.startswith('test_')}
for name, expected in [('web/python/test_noon_errors_wasm.py', 11), ('web/python/test_noon_callback_errors_wasm.py', 7)]:
    final = test_names(Path(name).read_text())
    assert len(final) == expected, (name, final)
    for ref in (CANDIDATE, BASE):
        before = subprocess.run(['git', 'show', f'{ref}:{name}'], text=True, capture_output=True)
        if before.returncode == 0:
            assert test_names(before.stdout) <= final, (name, ref)
player = 'crates/noon-web/src/semantic_execution_player.rs'
for ref in (CANDIDATE, BASE):
    before = set(re.findall(r'fn ([a-zA-Z0-9_]+)\(', git('show', f'{ref}:{player}')))
    after = set(re.findall(r'fn ([a-zA-Z0-9_]+)\(', Path(player).read_text()))
    assert before <= after, (ref, before - after)
assert 'expected_kind = "scalar" if kind == "scalar_signal" else kind' in Path('web/python/_manim_updaters.py').read_text()
assert 'impl From<CallbackReadRequestWire>' not in Path(player).read_text()
assert 'Family { node }' in Path(player).read_text()
assert 'assert.equal(report.python.additionalTests, 11)' in Path(runner).read_text()
assert 'assert.equal(report.python.callbackTests, 7)' in Path(runner).read_text()
assert 'assert.equal(report.advancement.length, 9)' in Path(runner).read_text()

subprocess.run(['cargo', 'fmt', '--all'], check=True)
subprocess.run(['git', 'add', '--', runner, 'crates/noon-web/src/authoring_error.rs', player, str(path)], check=True)
expected = {
    'crates/noon-web/src/authoring_error.rs', player,
    'crates/noon-web/src/semantic_execution_player/callback_error_tests.rs', runner,
    'web/python/_manim_updaters.py', 'web/python/test_updater_snapshot.py',
    'web/python/test_noon_callback_errors_wasm.py',
    'web/python/examples/ordinary_callback_sparse_reads.py',
}
assert set(git('diff', '--cached', '--name-only', BASE).splitlines()) == expected
assert not git('diff', '--name-only', '--diff-filter=U')
assert not git('diff', '--name-only')
print('Eight scoped paths; all independent tests and master production changes preserved.')
