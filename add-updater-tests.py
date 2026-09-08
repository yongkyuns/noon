from pathlib import Path
p = Path('.')
f = p / 'crates/noon/src/execution_session/publication/tests.rs'
f.write_text(f.read_text() + '''

#[test]
fn live_updater_removal_replacement_and_freeze_keep_one_runtime() {
    use crate::{HostCallbackId, RustHostCallbackTable};
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    store.attach_to_scene(node).unwrap();
    for _ in 0..1024 {
        let sibling = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 0.1 }));
        store.attach_to_scene(sibling).unwrap();
    }
    let forth = HostCallbackId::new(7);
    let back = HostCallbackId::new(8);
    let mut callbacks = RustHostCallbackTable::new();
    for (id, sign) in [(forth, 1.0), (back, -1.0)] {
        callbacks.insert(id, move |context| {
            let mut transform = context.target_state().transform;
            transform.translation.x += sign * context.delta_time() as f32;
            context.set_target_transform(transform)
        }).unwrap();
    }
    callbacks.add_updater(&mut store, node, forth, 0.0, None).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let runtime = session.runtime_identity();
    callbacks.advance_to(&mut session, 2.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
    session.take_frame_changes();
    let before = session.publication_context();
    let mut tx = SemanticMutationTransaction::new();
    tx.remove_updater(node, forth, 2.0);
    tx.add_updater(node, back, 2.0, None);
    session.apply_semantic_transaction(&mut store, tx).unwrap();
    assert_eq!(session.runtime_identity(), runtime);
    assert_eq!(session.frame().time, 2.0);
    assert_eq!(session.frame().objects[0].transform.translation.x, 2.0);
    assert_eq!(session.publication_context().scene_revision(), before.scene_revision().checked_next().unwrap());
    assert!(session.take_frame_changes().is_empty());
    assert_eq!(session.last_structural_publication_stats().preparation.object_states_lowered, 0);
    assert_eq!(session.runtime.last_patch_stats().full_seeks, 0);
    assert_eq!(session.runtime.last_patch_stats().objects_recomputed, 0);
    assert_eq!(session.runtime.last_patch_stats().full_group_rebuilds, 0);
    callbacks.advance_to(&mut session, 3.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 1.0);
    let mut clear = SemanticMutationTransaction::new();
    clear.clear_updaters(node, 3.0);
    session.apply_semantic_transaction(&mut store, clear).unwrap();
    callbacks.advance_to(&mut session, 4.0).unwrap();
    assert_eq!(session.frame().objects[0].transform.translation.x, 1.0,
        "removal must freeze the last effective value, not restore authored state");
    assert_eq!(session.runtime_identity(), runtime);
    assert_eq!(session.wake_state().timeline(), noon_runtime::TimelineWakeState::Quiescent);
    let before = session.publication_context();
    let mut noop = SemanticMutationTransaction::new();
    noop.remove_updater(node, back, 4.0);
    session.apply_semantic_transaction(&mut store, noop).unwrap();
    assert_eq!(session.publication_context(), before);
}

#[test]
fn live_updater_edits_reject_pending_phases_retroactivity_and_unindexed_targets_atomically() {
    use crate::{CallbackAdvance, HostCallbackId};
    let mut store = SemanticStore::new();
    let node = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    store.attach_to_scene(node).unwrap();
    let other = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    store.attach_to_scene(other).unwrap();
    let mut initial = SemanticMutationTransaction::new();
    initial.add_updater(node, HostCallbackId::new(1), 0.0, None);
    initial.apply(&mut store).unwrap();
    let mut session = ExecutionSession::from_semantic_store(&store).unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } = session.advance_to_callback_barrier(0.0).unwrap() else { panic!("initial phase") };
    let before = session.publication_context();
    let mut remove = SemanticMutationTransaction::new();
    remove.clear_updaters(node, 0.0);
    assert!(matches!(session.apply_semantic_transaction(&mut store, remove),
        Err(ExecutionSessionPublicationError::RequiredCallbackPending)));
    assert_eq!(store.scene_revision(), before.scene_revision());
    session.commit_required_callback_phase(overlay.finish()).unwrap();
    let CallbackAdvance::HostRequired { overlay, .. } = session.advance_to_callback_barrier(1.0).unwrap() else { panic!("next phase") };
    let stale = overlay.clone().finish();
    session.commit_required_callback_phase(overlay.finish()).unwrap();
    let before = session.publication_context();
    for (target, time) in [(node, 0.5), (other, 1.0)] {
        let mut tx = SemanticMutationTransaction::new();
        tx.add_updater(target, HostCallbackId::new(2), time, None);
        assert!(session.apply_semantic_transaction(&mut store, tx).is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(store.scene_revision(), before.scene_revision());
    }
    let mut remove = SemanticMutationTransaction::new();
    remove.clear_updaters(node, 1.0);
    session.apply_semantic_transaction(&mut store, remove).unwrap();
    assert!(session.commit_required_callback_phase(stale).is_err());
    assert!(matches!(session.advance_to_callback_barrier(1.0).unwrap(), CallbackAdvance::Ready(_)));
}
''')
f = p / 'crates/noon-web/src/canonical_authoring_scene.rs'
s = f.read_text().replace('fn callback_occurrences_publish_before_session_lowering_and_reject_late_edits()', 'fn callback_occurrences_publish_through_the_same_live_session()')
s = s.replace('''        let error = context
            .add_updater(&circle, HostCallbackId::new(13), 1.0, None)
            .unwrap_err();
        assert!(error.contains("before canonical execution begins"));''', '''        context.add_updater(&circle, HostCallbackId::new(13), 1.0, None).unwrap();
        context.remove_updater(&circle, HostCallbackId::new(12), 0.0).unwrap();
        context.clear_updaters(&circle, 1.0).unwrap();
        let registrations = context.scene.store().borrow()
            .semantic_updater_registrations(circle.node_id()).unwrap().to_vec();
        assert_eq!(registrations.len(), 2);
        assert_eq!(registrations[0].inactive_from(), Some(0.0));
        assert_eq!(registrations[1].inactive_from(), Some(1.0));''')
f.write_text(s)
f = p / 'crates/noon-compile/src/semantic_lowering/host_callbacks.rs'
s = f.read_text()
i = s.rfind('\n}')
s = s[:i] + '''
    #[test]
    fn staged_updater_revision_uses_semantic_order_and_does_not_publish_early() {
        let mut store = SemanticStore::new();
        let first = object(&mut store, 1.0);
        let second = object(&mut store, 2.0);
        store.attach_to_scene(first).unwrap();
        store.attach_to_scene(second).unwrap();
        add_updater(&mut store, first, 7, 0.0);
        add_updater(&mut store, second, 8, 0.0);
        let original = lower_semantic_host_callbacks(&store, &[first, second]);
        let before = store.scene_revision();
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_updater(first, HostCallbackId::new(7), 2.0);
        tx.add_updater(first, HostCallbackId::new(9), 2.0, None);
        let prepared = tx.prepare(&mut store).unwrap();
        let revised = original.prepare_registration_revision(&prepared, 2.0).unwrap().unwrap();
        assert_eq!(prepared.store().scene_revision(), before);
        assert_eq!(prepared.store().semantic_updater_registrations(first).unwrap().len(), 1);
        assert_eq!(revised.occurrences().iter().map(|item| (item.target(), item.callback_id())).collect::<Vec<_>>(),
            vec![(first, HostCallbackId::new(7)), (first, HostCallbackId::new(9)), (second, HostCallbackId::new(8))]);
        drop(prepared);
        assert_eq!(store.scene_revision(), before);
        assert_eq!(original.occurrences().len(), 2);
    }
''' + s[i:]
f.write_text(s)
f = p / 'crates/noon/src/example_scenes/live_updater_lifecycle.rs'
f.write_text('''//! Sequential counterpart of the exact Python RotationUpdater gallery source.
use crate::{ContinuationStep, HostCallbackId, LiveContinuation, LiveProgram, LiveSession,
    Mobject, RustHostCallbackTable, Scene, SemanticMutationTransaction, Vec2};

const FORTH: HostCallbackId = HostCallbackId::new(1);
const BACK: HostCallbackId = HostCallbackId::new(2);

pub struct RotationUpdater { moving: Mobject, stage: u8 }

impl LiveContinuation for RotationUpdater {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        match self.stage {
            0 => {}
            1 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.remove_updater(self.moving.node_id(), FORTH, 2.0);
                tx.add_updater(self.moving.node_id(), BACK, 2.0, None);
                live.apply(tx).map_err(|e| e.to_string())?;
            }
            2 => {
                let mut tx = SemanticMutationTransaction::new();
                tx.remove_updater(self.moving.node_id(), BACK, 4.0);
                live.apply(tx).map_err(|e| e.to_string())?;
            }
            3 => return Ok(ContinuationStep::Finished),
            _ => unreachable!(),
        }
        let duration = if self.stage == 2 { 0.5 } else { 2.0 };
        self.stage += 1;
        live.wait_segment(duration).map(ContinuationStep::Await).map_err(|e| e.to_string())
    }
}

pub fn program() -> Result<(LiveProgram<RotationUpdater>, RustHostCallbackTable), String> {
    let mut scene = Scene::new();
    let mut reference = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    reference.set_color(1.0, 1.0, 1.0, 1.0)?;
    let mut moving = scene.line((0.0, 0.0), (-1.0, 0.0))?;
    moving.set_color(1.0, 1.0, 0.0, 1.0)?;
    scene.add(&reference)?;
    scene.add(&moving)?;
    let mut callbacks = RustHostCallbackTable::new();
    for (id, sign) in [(FORTH, 1.0), (BACK, -1.0)] {
        callbacks.insert(id, move |context| {
            let transform = context.target_transform_rotated_about_point(
                sign * context.delta_time(), Vec2::ZERO,
            ).map_err(std::io::Error::other)?;
            context.set_target_transform(transform).map_err(std::io::Error::other)
        }).map_err(|e| e.to_string())?;
    }
    callbacks.add_updater(&mut scene.store().borrow_mut(), moving.node_id(), FORTH, 0.0, None)
        .map_err(|e| e.to_string())?;
    Ok((scene.into_live_program(RotationUpdater { moving, stage: 0 }).map_err(|e| e.to_string())?, callbacks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProgramStatus;

    #[test]
    fn live_rotation_updater_reverses_then_stops_without_replaying_source() {
        let (mut program, mut callbacks) = program().unwrap();
        let identity = program.session().runtime_identity();
        for end in [2.0, 4.0, 4.5] {
            assert!(matches!(program.resume().unwrap(), LiveProgramStatus::Awaiting(_)));
            let middle = if end == 2.0 { 1.0 } else if end == 4.0 { 3.0 } else { 4.25 };
            program.drive_to(&mut callbacks, middle).unwrap();
            let angle = program.session().frame().objects[1].transform.rotation;
            assert!((angle - if end == 4.5 { 0.0 } else { 1.0 }).abs() < 1e-5);
            if let LiveProgramStatus::PublicationPending(expected) = program.drive_to(&mut callbacks, end).unwrap() {
                let publication = program.take_renderer_publication().context();
                assert_eq!(expected, publication);
                program.admit_publication(publication).unwrap();
            }
            assert_eq!(program.session().runtime_identity(), identity);
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
        let frame = program.session().frame();
        assert_eq!(frame.time, 4.5);
        assert!((frame.objects[1].transform.rotation).abs() < 1e-5);
        assert_eq!(frame.objects.len(), 2);
    }
}
''')
f = p / 'crates/noon/src/example_scenes.rs'
s = f.read_text().replace('pub mod ordinary_membership;', 'pub mod ordinary_membership;\npub mod live_updater_lifecycle;', 1)
assert 'pub mod live_updater' in s
f.write_text(s)
(p / 'crates/noon-native/examples/ordinary_live_updater_lifecycle.rs').write_text('''fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::live_updater_lifecycle::program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}
''')
f = p / 'crates/noon-web/src/direct_execution_smoke.rs'
f.write_text(f.read_text() + '''
/// Sequential updater removal/replacement through the shared native/WASM session.
#[wasm_bindgen(js_name = createDirectLiveUpdaterLifecycleSmokeRenderer)]
pub async fn create_direct_live_updater_lifecycle_smoke_renderer(
    canvas: OffscreenCanvas,
) -> Result<WasmExecutionCanvasRenderer, JsValue> {
    let (program, callbacks) = noon::example_scenes::live_updater_lifecycle::program().map_err(js_error)?;
    WasmExecutionCanvasRenderer::create_from_live_program_with_callbacks(canvas, program, callbacks).await
}
''')
