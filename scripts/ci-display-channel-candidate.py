from pathlib import Path

p = Path('crates/noon-runtime/src/lib.rs')
s = p.read_text()
old = '''            self.relower_object(
                channel.object_index as usize,
                self.frame.time,
                &mut evaluation,
            );'''
new = '''            self.relower_timeline_object(
                channel.object_index as usize,
                &[Some(channel), None],
                &mut evaluation,
            );'''
assert s.count(old) == 1
s = s.replace(old, new)
old = '            self.relower_object(object_index, self.frame.time, &mut evaluation);'
assert s.count(old) == 1
s = s.replace(old, '''            self.relower_timeline_object(object_index, &affected_channels, &mut evaluation);''')
helper = '''    /// Visibility channels do not own affine or paint state. Re-evaluating one
    /// must preserve those independent effective domains, including values left
    /// by an updater that has already stopped. No callback history is needed here.
    fn relower_timeline_object(
        &mut self,
        object_index: usize,
        channels: &[Option<CompiledChannelKey>; 2],
        stats: &mut EvaluationStats,
    ) {
        let relevant = || channels.iter().flatten().filter(|channel| channel.object_index as usize == object_index);
        if relevant().any(|channel| !matches!(channel.property, Property::Presence | Property::Appearance | Property::Reveal)) {
            self.relower_object(object_index, self.frame.time, stats);
            return;
        }
        for channel in relevant() {
            match channel.property {
                Property::Presence => self.frame.presences[object_index] = initial_channel_bool(&self.compiled, object_index, Property::Presence, true),
                Property::Appearance => self.frame.objects[object_index].appearance = initial_channel_scalar(&self.compiled, object_index, Property::Appearance, 1.0),
                Property::Reveal => self.frame.reveals[object_index] = initial_channel_scalar(&self.compiled, object_index, Property::Reveal, 1.0),
                _ => unreachable!("visibility-only channels were checked above"),
            }
            self.reapply_properties(object_index, &[channel.property]);
            stats.groups_evaluated += self.last_stats.groups_evaluated;
            stats.tracks_advanced += self.last_stats.tracks_advanced;
            stats.binary_search_steps += self.last_stats.binary_search_steps;
        }
    }

'''
needle = '    fn reapply_properties(&mut self, object_index: usize, properties: &[Property]) {'
assert s.count(needle) == 1
s = s.replace(needle, helper + needle)
p.write_text(s)

Path('crates/noon-runtime/tests/display_channel_mutation.rs').write_text('''use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style, TrackDefinition,
    TrackId, TrackTiming, TrackValues, Transform2D, Vec2,
};
use noon_runtime::{EffectivePropertyWrite, SceneInstance};

fn instance_with_retained_effective_state() -> SceneInstance {
    let objects = (0..2).map(|id| CompiledObject::new(
        ObjectId::new(id), GeometryRef::circle(1.0), Transform2D::IDENTITY, Style::default(),
    )).collect();
    let mut instance = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    let prepared = instance.prepare_advance_to(0.0).unwrap();
    let effective = instance.prepare_effective_property_batch(&[
        EffectivePropertyWrite::Transform {
            object: ObjectId::new(0),
            transform: Transform2D { translation: Vec2::new(3.0, -2.0), rotation: 0.7, scale: Vec2::new(1.5, 0.75) },
        },
        EffectivePropertyWrite::Style { object: ObjectId::new(0), style: Style { opacity: 0.35, stroke_width: 7.0, ..Style::default() } },
    ]).unwrap();
    instance.commit_prepared_frame(prepared, effective).unwrap();
    // End the effective driver phase without overwriting its unowned domains.
    instance.advance_to(1.0).unwrap();
    instance.take_frame_changes();
    instance
}

fn track(property: Property) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(7), object: ObjectId::new(0), property,
        values: if property == Property::Presence { TrackValues::Bool { from: true, to: false } } else { TrackValues::Scalar { from: 1.0, to: 0.0 } },
        timing: TrackTiming::new(1.0, if property == Property::Presence { 0.0 } else { 1.0 }, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    }
}

#[test]
fn display_track_add_replace_and_remove_preserve_independent_effective_domains() {
    for property in [Property::Appearance, Property::Reveal, Property::Presence] {
        let mut instance = instance_with_retained_effective_state();
        let before = instance.frame().objects.clone();
        let assert_independent = |instance: &SceneInstance| {
            assert_eq!(instance.frame().objects[0].transform, before[0].transform);
            assert_eq!(instance.frame().objects[0].style, before[0].style);
            assert_eq!(instance.frame().objects[0].content, before[0].content);
            assert_eq!(instance.frame().objects[1], before[1]);
        };
        instance.apply_execution_patch(&ExecutionPatch::AddTrack(track(property))).unwrap();
        assert_independent(&instance);
        instance.advance_to(1.5).unwrap();
        assert_independent(&instance);
        match property {
            Property::Appearance => assert_eq!(instance.frame().objects[0].appearance, 0.5),
            Property::Reveal => assert_eq!(instance.frame().reveal(0), 0.5),
            Property::Presence => assert!(!instance.frame().is_present(0)),
            _ => unreachable!(),
        }
        let mut replacement = track(property);
        replacement.values = if property == Property::Presence { TrackValues::Bool { from: false, to: true } } else { TrackValues::Scalar { from: 0.0, to: 1.0 } };
        instance.apply_execution_patch(&ExecutionPatch::ReplaceTrack(replacement)).unwrap();
        assert_independent(&instance);
        instance.advance_to(2.0).unwrap();
        assert_independent(&instance);
        instance.apply_execution_patch(&ExecutionPatch::RemoveTrack(TrackId::new(7))).unwrap();
        assert_independent(&instance);
        assert_eq!(instance.frame().objects[0].appearance, 1.0);
        assert_eq!(instance.frame().reveal(0), 1.0);
        assert!(instance.frame().is_present(0));
        assert_eq!(instance.last_patch_stats().objects_recomputed, 1);
        assert_eq!(instance.last_patch_stats().full_seeks, 0);
        assert_eq!(instance.take_frame_changes().object_indices(), &[0]);
    }
}

#[test]
fn display_channel_reconciliation_preserves_independent_effective_domains() {
    for property in [Property::Appearance, Property::Reveal] {
        let mut instance = instance_with_retained_effective_state();
        let before = instance.frame().objects[0].clone();
        instance.apply_execution_patch(&ExecutionPatch::AddTrack(track(property))).unwrap();
        instance.advance_to(2.0).unwrap();
        instance.apply_execution_patch(&ExecutionPatch::ReconcileTrack {
            track: TrackId::new(7), object: ObjectId::new(0), property, end_time: 2.0,
        }).unwrap();
        assert_eq!(instance.frame().objects[0].transform, before.transform);
        assert_eq!(instance.frame().objects[0].style, before.style);
        assert_eq!(instance.frame().objects[0].appearance, 1.0);
        assert_eq!(instance.frame().reveal(0), 1.0);
    }
}
''')
