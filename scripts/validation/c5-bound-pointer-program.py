"""Staging-only regression insertion for the consumed live-program input path."""
from pathlib import Path
p = Path('crates/noon/src/live_session/pointer_actions/tests.rs')
s = p.read_text()
old = 'started(&click(&scene, &mut cloned, 0, 0.0));'
assert s.count(old) == 1
p.write_text(s.replace(old, '// A clone retains the last admitted sequence; changing its pointer is not a reset.\n    started(&click(&scene, &mut cloned, 4, 0.0));'))
p = Path('crates/noon/src/live_program/property_animation/tests.rs')
s = p.read_text()
assert 'accepted_bound_click_refreshes_endpoint_without_resuming_source' not in s
p.write_text(s + r'''

#[test]
fn accepted_bound_click_refreshes_endpoint_without_resuming_source() {
    use crate::{PointerClickActionOutcome, SemanticPointerClickAction};
    use noon_core::{NativeInputModifiers, NativePointerId, NativePointerInput,
        NativePointerInputKind, NativePointerPosition, Vec2};
    let (mut program, indicated) = fixture();
    // Circle constructors are outline-only. Fill picking must observe a visible fill.
    program.scene.owned_live().set_fill(&indicated, 0.0, 0.5, 1.0, 1.0).unwrap();
    program.scene.owned_live().set_pointer_click_action(
        &indicated, Some(SemanticPointerClickAction::default()),
    ).unwrap();
    program.set_pointer_fill_clicks(Some(5.0)).unwrap();
    program.configure_native_pointer_input(NativePointerId { source: 8, pointer: 3 }, 1).unwrap();
    program.resume().unwrap();
    program.drive_to(&mut RustHostCallbackTable::new(), 1.0).unwrap();
    let old_endpoint = program.take_renderer_publication().context();
    let before = program.session().frame().clone();
    let mut effect = None;
    for (sequence, down) in [(0, true), (1, false)] {
        let token = program.native_pointer_input_token().unwrap();
        let position = NativePointerPosition::new(Vec2::ZERO, Vec2::new(100.0,100.0)).unwrap();
        let kind = if down { NativePointerInputKind::Press { position, button: 0 } }
            else { NativePointerInputKind::Release { position, button: 0 } };
        let input = NativePointerInput::new(sequence, token.pointer(), token.context(), NativeInputModifiers::default(), kind);
        let result = program.submit_pointer_input_with_actions(&token, input).unwrap();
        if down { assert_eq!(result.action().unwrap(), PointerClickActionOutcome::None); }
        else {
            let PointerClickActionOutcome::Started(token) = result.action().unwrap() else { panic!("missing click effect: {result:?}") };
            effect = Some(token);
        }
    }
    let effect = effect.unwrap();
    assert_eq!(program.continuation.resumes, 1);
    assert!(program.admit_publication(old_endpoint).is_err());
    assert!(program.session().pointer_selection_highlight().is_none());
    program.advance_property_animations_by(0.5).unwrap();
    assert_eq!(program.session().frame().time, 1.0);
    assert_eq!(program.session().property_animation_elapsed(effect), Some(0.5));
    let current = program.take_renderer_publication().context();
    program.admit_publication(current).unwrap();
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    program.advance_property_animations_by(0.5).unwrap();
    assert_eq!(program.continuation.resumes, 2);
    assert!(!program.session().has_property_animations());
    assert_eq!(program.session().frame(), &before);
}
''')
manifest = Path('/tmp/c5-click-proof/paths.txt')
paths = set(manifest.read_text().splitlines())
paths.add(str(p))
manifest.write_text(''.join(path+'\n' for path in sorted(paths)))
