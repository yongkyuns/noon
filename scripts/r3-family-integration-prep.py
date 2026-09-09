"""Development-only deterministic integration of #1350 with its settled parent.

Reads exact Git revisions. No shared producer, admission or ownership changes.
The caller owns checkout, merge commit creation, tests and non-force publication.
"""
from pathlib import Path
import os
import subprocess

OLD = '0e69c0754d2ffb853037ef46825aeab7f298790d'
PARENT = '47bfd0b6b20ac4b828607dc32a655dca8060db71'
BASE = os.environ['BASE']


def source(ref, path):
    return subprocess.check_output(['git', 'show', f'{ref}:{path}'], text=True)


def once(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new, 1)


def function(text, name):
    start = text.index(f'    pub fn {name}(')
    end = text.index('\n    }', start) + len('\n    }')
    return text[start:end]


# Retain the landed family projection; replace only its placeholder callback cause.
p = 'crates/noon-web/src/authoring_error.rs'
text = source(BASE, p)
old = source(OLD, p)
start = old.index('// Callback transactions already have typed shared errors.')
end = old.index('impl From<ExecutionSessionPublicationError>', start)
text = once(text, '    ExecutionSessionPublicationError, LiveSessionError,',
            '    ExecutionSessionCallbackError, ExecutionSessionCallbackReadError,\n'
            '    ExecutionSessionPublicationError, LiveSessionError,')
text = once(text, 'impl From<ExecutionSessionPublicationError>', old[start:end] + 'impl From<ExecutionSessionPublicationError>')
text = once(text, 'Callback(e) => Self::unclassified("callback.family.read", &e),',
            'Callback(e) => Self::caused_by("callback.family.read", e.to_string(), e.into()),')
Path(p).write_text(text)

# Keep master imports/wire/cfg and all guards; transplant the already-qualified
# typed forwarding block, using master's complete Family-aware read body.
p = 'crates/noon-web/src/semantic_execution_player.rs'
text = source(BASE, p)
old = source(OLD, p)
start = old.index('// Keep the shared callback failures typed until the actual JS boundary.')
end = old.index('#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]', start)
block = old[start:end]
read = function(text, 'required_callback_read_json')
read = once(read, 'Result<String, String>', 'Result<String, AuthoringFailure>')
read = once(read,
    '.required_callback_family_read(&store.borrow(), token, node.into())\n                    .map_err(|error| error.to_string())?',
    '.required_callback_family_read(&store.borrow(), token, node.into())\n                    .map_err(AuthoringFailure::from)?')
read = once(read,
    '.required_callback_read(token, request)\n            .map_err(|error| error.to_string())?',
    '.required_callback_read(token, request)\n            .map_err(AuthoringFailure::from)?')
assert read.count('.map_err(|error| error.to_string())') == 2
read = read.replace('.map_err(|error| error.to_string())',
                    '.map_err(|error| AuthoringFailure::from(error.to_string()))')
block = once(block, function(block, 'required_callback_read_json'),
             '    #[cfg(any(target_arch = "wasm32", test))]\n' + read)
text = once(text, '#[cfg(any(target_arch = "wasm32", test))]\nuse crate::authoring_error::AuthoringFailure;',
            'use crate::authoring_error::AuthoringFailure;')
for name, js_name in [
    ('required_callback_read_json', 'requiredCallbackReadJson'),
    ('commit_callback_phase_json', 'commitCallbackPhaseJson'),
    ('fail_callback_phase_json', 'failCallbackPhaseJson'),
    ('interrupt_callback_phase_json', 'interruptCallbackPhaseJson'),
]:
    attr = f'    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = {js_name}))]'
    # The actual JS entrypoint stays WASM-only; the typed read keeps master cfg.
    prefix = '    #[cfg(any(target_arch = "wasm32", test))]\n' if name == 'required_callback_read_json' else ''
    text = once(text, prefix + attr + '\n' + function(text, name),
                '    #[cfg(target_arch = "wasm32")]\n' + attr + '\n' + function(old, name + '_wasm'))
anchor = '#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]\nimpl SemanticExecutionPlayer {\n    fn callback_phase_json('
text = once(text, anchor, block + anchor)
assert 'impl From<CallbackReadRequestWire>' not in text
assert 'mod callback_error_tests;' not in text
text += '\n#[cfg(test)]\nmod callback_error_tests;\n'
Path(p).write_text(text)

# Replay only the three original exception-forwarding edits, keeping Family's
# response kind, family paint helper and all scalar dispatch from master.
p = 'web/python/_manim_updaters.py'
text = source(BASE, p)
text = once(text, 'import noon as _base\n', 'import noon as _base\nfrom _noon_errors import engine_await, raise_engine_error\n')
text = once(text, 'raise RuntimeError(f"canonical callback sparse read failed: {error}") from None',
            'raise_engine_error(error, operation="callback.read")')
text = once(text, 'raise RuntimeError(f"canonical callback sparse read failed: {self._prefetch_errors[key]}") from None',
            'raise_engine_error(self._prefetch_errors[key], operation="callback.read")')
text = once(text, '''        raw = await noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        )''', '''        raw = await engine_await(noonReadSemanticContinuationCallback(
            self._authoring_context,
            json.dumps(self.token, separators=(",", ":")),
            json.dumps(request, separators=(",", ":")),
        ), operation="callback.read")''')
assert 'expected_kind = "scalar" if kind == "scalar_signal" else kind' in text
Path(p).write_text(text)

# Keep the landed wait test/count, plus the callback suite. This explicit check
# rejects an unanticipated runner change instead of choosing ours/theirs.
p = 'scripts/typed-authoring-errors-smoke.mjs'
assert once(source(PARENT, p), 'assert.equal(report.python.additionalTests, 9);',
            'assert.equal(report.python.additionalTests, 10);') == source(BASE, p)
text = once(source(OLD, p), 'assert.equal(report.python.additionalTests, 9);',
            'assert.equal(report.python.additionalTests, 10);')
text = once(text, 'assert.equal(report.python.callbackTests, 6);',
            'assert.equal(report.python.callbackTests, 7);')
Path(p).write_text(text)

# Actual production-worker sparse read: verify the typed chain while keeping
# existing once-per-phase side effects, staged alpha and final position checks.
p = 'web/python/examples/ordinary_callback_sparse_reads.py'
text = source(BASE, p)
text = once(text, 'import _manim_updaters\n', 'import _manim_updaters\nfrom _noon_errors import NoonStaleHandleError\n')
text = once(text, '''            except RuntimeError:
                pass  # The detached final member cannot be read from this phase.''', '''            except NoonStaleHandleError as error:
                assert error.category == "stale_handle"
                assert error.code == "callback.family.read"
                assert error.operation == "callback.read"
                assert error.rust_cause.code == "callback.read"
                assert error.rust_cause.cause.code == "callback_read.unknown_object"
                assert error.__cause__ is not None''')
Path(p).write_text(text)

# A Family read can visit a valid first leaf before the non-live final leaf.
# Exercise this on real WASM without changing the existing fixture by default.
p = 'web/python/test_noon_callback_errors_wasm.py'
text = source(OLD, p)
text = once(text, 'def __init__(self, with_tracker=False):', 'def __init__(self, with_tracker=False, with_families=False):')
text = once(text, '        self.context.bindMobject("0", self.target)', '''        self.families = []
        if with_families:
            for handles in ((self.target, self.detached), (self.target,)):
                members = wasm.WasmSceneMembershipBatch.new("add")
                for handle in handles:
                    members.appendMobject("", handle)
                self.families.append(self.store.createFamily(members))
        self.context.bindMobject("0", self.target)''')
text = once(text, '        self.store.free()', '        for family in self.families:\n            family.free()\n        self.store.free()')
text = once(text, '    def test_valid_abort_remains_terminal_and_repeat_keeps_no_pending_precedence(self):', '''    def test_family_read_preserves_nested_cause_and_same_player_recovery(self):
        fixture = CallbackFixture(with_families=True)
        self.addCleanup(fixture.close)
        token = json.dumps(fixture.phase["token"])
        def request(handle):
            return json.dumps({"kind": "family", "node": {
                "slot": handle.semanticSlot, "generation": handle.semanticGeneration,
            }})
        error = self.rejection(fixture, fixture.player.requiredCallbackReadJson,
                               [token, request(fixture.families[0])],
                               NoonStaleHandleError, "stale_handle", "callback.family.read")
        self.assertEqual(error.rust_cause.code, "callback.read")
        self.assertEqual(error.rust_cause.cause.code, "callback_read.unknown_object")
        self.assertIsNone(error.rust_cause.cause.cause)
        self.assertEqual(error.rust_cause.category, "stale_handle")
        self.assertEqual(error.rust_cause.cause.category, "stale_handle")
        before = fixture.state()
        value = json.loads(engine_call(fixture.player.requiredCallbackReadJson,
                           token, request(fixture.families[1]), operation="callback.read"))
        self.assertEqual(value["kind"], "family")
        self.assertEqual(value["objects"], fixture.phase["objects"])
        self.assertEqual(fixture.state(), before)
        self.assertIsNone(fixture.player.drainDeltaJson())
        fixture.finish()

    def test_valid_abort_remains_terminal_and_repeat_keeps_no_pending_precedence(self):''')
Path(p).write_text(text)

p = 'crates/noon-web/src/semantic_execution_player/callback_error_tests.rs'
text = source(OLD, p)
text += '''
#[test]
fn family_read_rejection_preserves_nested_cause_and_the_same_pending_phase() {
    let mut scene = noon::Scene::new();
    let target = scene.circle(0.5).unwrap();
    let detached = scene.circle(0.2).unwrap();
    let mixed = scene.family(&[(&target).into(), (&detached).into()]).unwrap();
    let live = scene.family(&[(&target).into()]).unwrap();
    scene.add(&target).unwrap();
    let mut transaction = SemanticMutationTransaction::new();
    transaction.add_updater(target.node_id(), HostCallbackId::new(1), 0.0, None);
    transaction.apply(&mut scene.integration_store().borrow_mut()).unwrap();
    let mut player = SemanticExecutionPlayer::from_live_session(
        scene.execution_session().unwrap(),
        std::rc::Rc::clone(scene.integration_store()), scene.root(), 1.0, 41,
    ).unwrap();
    let phase = phase(&mut player);
    player.initial_delta_json().unwrap();
    let before = state(&player);
    let revision = scene.revision();
    let request = |node: SemanticNodeId| json!({"kind": "family", "node": {
        "slot": node.slot(), "generation": node.generation(),
    }}).to_string();
    let error = player.required_callback_read_json(
        &phase["token"].to_string(), &request(mixed.node_id()),
    ).unwrap_err();
    assert_eq!(error.category, "stale_handle");
    assert_eq!(error.code, "callback.family.read");
    let callback = error.cause.as_ref().unwrap();
    assert_eq!(callback.code, "callback.read");
    let read = callback.cause.as_ref().unwrap();
    assert_eq!(read.code, "callback_read.unknown_object");
    assert!(read.cause.is_none());
    assert!(error.source().unwrap().source().is_some());
    assert_eq!(state(&player), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    let value: Value = serde_json::from_str(&player.required_callback_read_json(
        &phase["token"].to_string(), &request(live.node_id()),
    ).unwrap()).unwrap();
    assert_eq!(value["kind"], "family");
    assert_eq!(value["objects"], phase["objects"]);
    assert_eq!(state(&player), before);
    assert_eq!(scene.revision(), revision);
    assert_eq!(player.drain_delta_json().unwrap(), None);
    finish(&mut player, Some(&phase));
}
'''
Path(p).write_text(text)

# This already-reviewed unit-test adjustment has no overlap with master.
p = 'web/python/test_updater_snapshot.py'
assert source(PARENT, p) == source(BASE, p)
Path(p).write_text(source(OLD, p))

expected = {
    'crates/noon-web/src/authoring_error.rs',
    'crates/noon-web/src/semantic_execution_player.rs',
    'crates/noon-web/src/semantic_execution_player/callback_error_tests.rs',
    'scripts/typed-authoring-errors-smoke.mjs',
    'web/python/_manim_updaters.py',
    'web/python/test_updater_snapshot.py',
    'web/python/test_noon_callback_errors_wasm.py',
    'web/python/examples/ordinary_callback_sparse_reads.py',
}
subprocess.run(['git', 'add', '--', *sorted(expected)], check=True)
changed = set(subprocess.check_output(['git', 'diff', '--cached', '--name-only', BASE], text=True).splitlines())
assert changed == expected, (changed, expected)
assert not subprocess.check_output(['git', 'diff', '--name-only', '--diff-filter=U'], text=True)
print('Exactly eight scoped product paths staged; all other master content retained.')
