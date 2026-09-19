from pathlib import Path
import sys

if sys.argv[1] == 'tests':
    p = Path('crates/noon-runtime/src/replay/input_tests.rs')
    text = p.read_text()
    assert 'fn signal_only_input_rejects_a_previously_prepared_phase' not in text
    text += '''
#[test]
fn signal_only_input_rejects_a_previously_prepared_phase() {
    let (mut runtime, signal, _) = fixture();
    let stale = runtime.prepare_advance_to_with_reactive_inputs(
        0.0, &[(signal, ReactiveValue::Scalar(3.0))],
    ).unwrap();
    let effective = runtime.prepare_effective_property_batch(&[]).unwrap();
    let before = runtime.publication_context();
    runtime.advance_to_with_reactive_inputs(
        0.0, &[(signal, ReactiveValue::Scalar(2.0))],
    ).unwrap();
    let current = runtime.publication_context();
    assert_ne!(current, before, "signal-only changes are coherent publications too");
    assert!(matches!(runtime.commit_prepared_frame(stale, effective),
        Err(crate::PreparedFrameCommitError::StalePublication { .. })));
    assert_eq!(runtime.publication_context(), current);
    assert_eq!(runtime.reactive_value(signal), Some(&ReactiveValue::Scalar(2.0)));
    assert!(runtime.take_frame_changes().is_empty(), "no geometry changed");
}
'''
    p.write_text(text)
elif sys.argv[1] == 'source':
    p = Path('crates/noon-runtime/src/prepared_frame.rs')
    text = p.read_text()
    old = '''        if prepared.reactive.as_ref().is_some_and(|update| !update.is_empty()) {
            self.invalidate_replay_input();'''
    new = '''        let reactive_changed = prepared.reactive.as_ref().is_some_and(|update| !update.is_empty());
        if reactive_changed {
            self.invalidate_replay_input();'''
    assert text.count(old) == 1
    text = text.replace(old, new)
    old = '''        if time_changed || changed {
            self.publication = self.publication.with_frame_epoch('''
    new = '''        if time_changed || changed || reactive_changed {
            self.publication = self.publication.with_frame_epoch('''
    assert text.count(old) == 1
    p.write_text(text.replace(old, new))
else:
    raise ValueError('expected tests or source')
