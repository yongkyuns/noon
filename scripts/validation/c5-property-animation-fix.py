"""Staging only: preserve coherent base state during independent driver ticks."""
from pathlib import Path

def replace(path, old, new, count=1):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == count, (path, old, s.count(old))
    p.write_text(s.replace(old, new))

p = 'crates/noon-runtime/src/prepared_frame.rs'
replace(p, '/// Sparse, unpublished timeline/native evaluation for a required callback phase.',
'''/// Sparse, unpublished effective frame. Ordinary timeline/native evaluation
/// and samples that preserve the already-coherent base share this publication.''')
replace(p, '    base_time: f64,\n    time: f64,',
'''    base_time: f64,
    time: f64,
    resample_base: bool,''')
replace(p, '            base_time: self.frame.time,\n            time,',
'''            base_time: self.frame.time,
            time,
            resample_base: true,''')
replace(p, '    /// Validate every effective write against the current execution shape before',
'''    /// Preserve the coherent base for an independent effective-only sample.
    /// In particular, do not run unrelated timeline groups, reactive bindings,
    /// host outputs or family endpoints again merely because a driver ticks.
    pub(crate) fn prepare_current_effective_frame(&self) -> Result<PreparedFrameEvaluation, EvaluationError> {
        if self.replay_is_sealed() { return Err(EvaluationError::ReplaySealed); }
        Ok(PreparedFrameEvaluation {
            runtime: self.identity,
            expected: self.publication,
            base_time: self.frame.time,
            time: self.frame.time,
            resample_base: false,
            requested_channels: Vec::new(),
            requested_family_animations: Vec::new(),
            cursor_updates: Vec::new(),
            rows: Vec::new(),
            stats: EvaluationStats::default(),
            scheduler_stats: TimelineSchedulerStats::default(),
            prior_driver_rows: 0,
            property_animations: self.prepare_property_animations(0.0)?,
            reactive: None,
        })
    }

    /// Validate every effective write against the current execution shape before''')
replace(p, '''        self.timeline_scheduler.advance(prepared.time);
        debug_assert_eq!(
            self.timeline_scheduler.requested(),
            prepared.requested_channels
        );
        debug_assert_eq!(
            self.timeline_scheduler.requested_family_animations(),
            prepared.requested_family_animations
        );''', '''        if prepared.resample_base {
            self.timeline_scheduler.advance(prepared.time);
            debug_assert_eq!(
                self.timeline_scheduler.requested(),
                prepared.requested_channels
            );
            debug_assert_eq!(
                self.timeline_scheduler.requested_family_animations(),
                prepared.requested_family_animations
            );
        }''')
replace(p, '        let mut changed = self.update_requested_family_animations(prepared.time);',
'''        let mut changed = prepared.resample_base
            && self.update_requested_family_animations(prepared.time);''')
replace(p, '        self.effective_driver_rows = next_drivers;',
'''        if prepared.resample_base {
            self.effective_driver_rows = next_drivers;
        } else {
            self.effective_driver_rows.extend(next_drivers);
        }''')
p = 'crates/noon-runtime/src/property_animation.rs'
replace(p, 'self.prepare_advance_to(self.frame.time).map_err(E::Evaluation)?',
           'self.prepare_current_effective_frame().map_err(E::Evaluation)?', count=2)
replace(p, '''            // A whole-object Transform and a Morph can own the affine render frame
            // even when the requested component has no individual timeline track.''',
'''            // Transform/morph/reveal can own the affine render frame, and a
            // presence track can retire the target, even without a component track.''')

p = Path('crates/noon-runtime/src/property_animation/tests.rs')
s = p.read_text()
s += '''
#[test]
fn effect_only_ticks_preserve_unrelated_host_override_until_authored_time_moves() {
    let mut runtime = instance(2, &[
        moving(2, Property::Position, TrackValues::Vec2 { from: Vec2::ZERO, to: Vec2::new(8.0, 0.0) }),
    ]);
    let token = runtime.start_restoring_property_animation(&[scale(1)]).unwrap();
    let phase = runtime.prepare_advance_to(0.5).unwrap();
    let effective = runtime.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Translation { object: ObjectId::new(2), translation: Vec2::new(99.0, 4.0) },
    ]).unwrap();
    runtime.commit_prepared_frame(phase, effective).unwrap();
    runtime.take_frame_changes();
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame().objects[1].transform.translation, Vec2::new(99.0, 4.0),
        "independent effect sample must not re-run an unrelated authored driver");
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    assert_eq!(runtime.take_frame_changes().object_indices(), &[0]);
    runtime.cancel_property_animation(token).unwrap();
    assert_eq!(runtime.frame().objects[1].transform.translation, Vec2::new(99.0, 4.0));
    assert_eq!(runtime.frame().time, 0.5);
    runtime.advance_to(1.0).unwrap();
    assert_eq!(runtime.frame().objects[1].transform.translation, Vec2::new(4.0, 0.0),
        "normal authored advancement must still re-evaluate the retained driver");
}

#[test]
fn effect_sample_cost_does_not_follow_unrelated_active_timeline_groups() {
    let tracks: Vec<_> = (2..=1000).map(|object| {
        let mut track = moving(object, Property::Position,
            TrackValues::Vec2 { from: Vec2::ZERO, to: Vec2::new(8.0, 0.0) });
        track.id = TrackId::new(object);
        track
    }).collect();
    let mut runtime = instance(1000, &tracks);
    runtime.advance_to(0.5).unwrap();
    runtime.take_frame_changes();
    runtime.start_restoring_property_animation(&[scale(1)]).unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.last_stats().groups_evaluated, 0);
    assert_eq!(runtime.take_frame_changes().object_indices(), &[0]);
    assert_eq!(runtime.frame().time, 0.5);
    assert_eq!(runtime.frame().objects[999].transform.translation, Vec2::new(2.0, 0.0));
}
'''
p.write_text(s)

# Explicit seeks retire drivers even when both authored time and signal values
# remain equal; old prepared samples must still be invalidated by a fresh epoch.
p = 'crates/noon-runtime/src/reactive/runtime.rs'
replace(p, '''        let effective_changed = !prepared.is_empty();
        if effective_changed {''', '''        let had_property_animations = self.has_property_animations();
        if had_property_animations && self.publication.frame_epoch().checked_next().is_none() {
            return Err(crate::EvaluationError::FrameEpochExhausted(self.publication.frame_epoch()));
        }
        let effective_changed = !prepared.is_empty();
        if effective_changed {''')
replace(p, '        if self.frame.time != previous_time || effective_changed {',
           '        if self.frame.time != previous_time || effective_changed || had_property_animations {')
p = 'crates/noon-runtime/src/lib.rs'
replace(p, '''        let had_property_animations = self.has_property_animations();
        self.seek_unchecked(time);''', '''        let had_property_animations = self.has_property_animations();
        if had_property_animations && self.publication.frame_epoch().checked_next().is_none() {
            return Err(EvaluationError::FrameEpochExhausted(self.publication.frame_epoch()));
        }
        self.seek_unchecked(time);''')

p = Path('crates/noon-runtime/src/property_animation/tests.rs')
p.write_text(p.read_text() + '''
#[test]
fn unchanged_reactive_seek_retires_effect_and_invalidates_its_prepared_sample() {
    use noon_compile::{lower_semantic_execution, SemanticExecutionIndex};
    use noon_core::{SemanticStore, SemanticObjectState, StoredGeometry,
        SemanticVec3, SemanticObjectProperty, ReactiveValue};
    let mut store = SemanticStore::new();
    let target = store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 }));
    store.attach_to_scene(target).unwrap();
    let signal = store.insert_semantic_input_signal(SemanticVec3::ZERO).unwrap();
    store.bind_semantic_signal(signal, target, SemanticObjectProperty::Translation).unwrap();
    let mut index = SemanticExecutionIndex::new();
    let lowered = lower_semantic_execution(&store, &mut index).unwrap();
    let signal = lowered.reactive().execution_signal_id(signal).unwrap();
    let object = index.execution_object_id(target).unwrap();
    let mut runtime = SceneInstance::from_semantic_execution(lowered);
    let mut track = scale(1); track.object = object;
    let token = runtime.start_restoring_property_animation(&[track]).unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    let prepared = runtime.prepare_advance_to(0.0).unwrap();
    let batch = runtime.prepare_effective_property_batch(&[]).unwrap();
    let before = runtime.publication_context();
    runtime.seek_with_reactive_inputs(0.0, &[(signal, ReactiveValue::from(Vec2::ZERO))]).unwrap();
    assert!(runtime.property_animation_elapsed(token).is_none());
    assert_ne!(runtime.publication_context(), before);
    assert_eq!(runtime.effective_object(object).unwrap().transform.scale, Vec2::ONE);
    assert!(matches!(runtime.commit_prepared_frame(prepared, batch),
        Err(PreparedFrameCommitError::StalePublication { .. })));
}

#[test]
fn separate_claims_on_one_object_release_without_retiring_each_other() {
    let mut runtime = instance(1, &[]);
    let mut fill = scale(1);
    fill.property = Property::Fill;
    fill.values = TrackValues::Color { from: Some(Color::BLUE), to: Some(Color::YELLOW) };
    let shape = runtime.start_restoring_property_animation(&[scale(1)]).unwrap();
    let paint = runtime.start_restoring_property_animation(&[fill]).unwrap();
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame().objects[0].style.fill, Some(Color::YELLOW));
    runtime.cancel_property_animation(shape).unwrap();
    assert_eq!(runtime.frame().objects[0].transform.scale, Vec2::ONE);
    assert_eq!(runtime.frame().objects[0].style.fill, Some(Color::YELLOW));
    assert_eq!(runtime.property_animation_elapsed(paint), Some(0.5));
    runtime.advance_property_animations_by(0.5).unwrap();
    assert_eq!(runtime.frame().objects[0].style.fill, Some(Color::BLUE));
    assert!(!runtime.has_property_animations());
}
''')
print('Independent samples preserve coherent base and remain local to active effects')
