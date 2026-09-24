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
print('Independent samples preserve coherent base and remain local to active effects')
