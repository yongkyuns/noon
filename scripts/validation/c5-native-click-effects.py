from pathlib import Path
import base64
import hashlib
import subprocess
import zlib

payload = ''.join(Path(f'scripts/validation/c5-native-effects-{i}.b64').read_text().strip() for i in range(3))
assert len(payload) == 11280
patch = zlib.decompress(base64.b64decode(payload, validate=True))
assert hashlib.sha256(patch).hexdigest() == '46a3c2aefcf523c34a6bddf036f2abf63d2d6cec943b53642d90164c2ff87e02'
proof = Path('/tmp/c5-native-click-proof')
proof.mkdir(exist_ok=True)
patch_path = proof / 'input.patch'
patch_path.write_bytes(patch)
subprocess.run(['git', 'apply', '--check', str(patch_path)], check=True)
subprocess.run(['git', 'apply', str(patch_path)], check=True)
paths = sorted(line.split(' b/', 1)[1] for line in patch.decode().splitlines() if line.startswith('diff --git '))
assert len(paths) == 10
assert all(path.startswith('crates/') or path == '.github/workflows/native-host-smoke.yml' for path in paths)
(proof / 'paths.txt').write_text('\n'.join(paths) + '\n')

def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))

replace('crates/noon/src/live_session/pointer_actions.rs',
    '/// Runtime declined an overlapping claim. No elapsed time or owner changed.',
    '/// Runtime declined an overlapping claim without restarting its owner.\n    /// Any separately supplied preceding effect interval has already been delivered.')
path = 'crates/noon-native/src/property_animation/tests.rs'
replace(path, '-std::f64::consts::LN_2 * 500.0 * scale', 'std::f64::consts::LN_2 * 500.0 * scale')
replace(path, 'fn fixture() -> (NativeApp, Rc<Cell<usize>>) {\n    let mut scene', '''fn fixture() -> (NativeApp, Rc<Cell<usize>>) {
    fixture_with_action(SemanticPointerClickAction::default(), 1.0)
}

fn fixture_with_action(action: SemanticPointerClickAction, scale: f64) -> (NativeApp, Rc<Cell<usize>>) {
    let mut scene''')
replace(path, 'object.set_translation(x, 0.0).unwrap();', 'object.set_translation(x, 0.0).unwrap();\n        object.set_scale(scale, scale).unwrap();')
replace(path, 'object.set_pointer_click_action(Some(SemanticPointerClickAction::default())).unwrap();', 'object.set_pointer_click_action(Some(action)).unwrap();')
p = Path(path)
p.write_text(p.read_text() + '''
#[test]
fn post_admission_action_failure_commits_native_sequence_before_surfacing_error() {
    // The authored factor is representable, but applying it to a scaled object
    // exceeds the render domain. Picking/input remain valid; action lowering fails.
    let action = SemanticPointerClickAction::indicate(f64::from(f32::MAX), noon_core::YELLOW, 1.0);
    let (mut app, _) = fixture_with_action(action, 2.0);
    present(&mut app);
    let now = Instant::now();
    occurrence_at(&mut app, 0.0, true, now).unwrap();
    let sequence = app.next_input_sequence;
    assert!(occurrence_at(&mut app, 0.0, false, now).is_err());
    assert_eq!(app.next_input_sequence, sequence + 1);
    assert!(!app.execution.property_animation_pending());
    assert!(app.effect_previous_tick.is_none());
    assert_eq!(app.session().frame().time, 0.0);
    let frame = app.pointer.presented.as_ref().unwrap();
    let token = frame.input_token(app.session(), frame.view()).unwrap();
    let position = frame.position(Vec2::new(400.0, 200.0)).unwrap();
    let repeated = NativePointerInput::new(sequence, token.pointer(), token.context(),
        NativeInputModifiers::default(), NativePointerInputKind::Release { position, button: 0 });
    assert!(app.submit_pointer_occurrence_at(&token, repeated, sequence + 1, now).is_err());
    assert_eq!(app.next_input_sequence, sequence + 1);
}
''')
