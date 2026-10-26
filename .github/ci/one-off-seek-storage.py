#!/usr/bin/env python3
"""Apply the narrow #2317 seek-storage repair to its exact reviewed source."""
import hashlib
from pathlib import Path

PATH = Path('crates/noon-runtime/src/lib.rs')
EXPECTED = 'ecea0c82225a57f0356808347f11fd35cfb7b055'
raw = PATH.read_bytes()
actual = hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest()
if actual != EXPECTED:
    raise SystemExit(f'refusing unreviewed lib.rs: {actual}, expected {EXPECTED}')
source = raw.decode()
start = source.index('fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {')
end = source.index('const PROPERTY_ORDER:', start)
old_initializer = source[start:end]
assert old_initializer.count('fn ') == 3, 'unexpected initializer region'
assert old_initializer.count('fn initial_bool_property(') == 1
assert old_initializer.count('fn initial_scalar_property(') == 1
new_initializer = '''fn base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    let mut frame = FrameState {
        time,
        objects: Vec::new(),
        presences: Vec::new(),
        reveals: Vec::new(),
        morphs: Vec::new(),
        render_geometries: Vec::new(),
        render_transforms: Vec::new(),
        family_animations: Vec::new(),
        family_animation_plan_indices: Vec::new(),
    };
    reset_base_frame(compiled, &mut frame, time);
    frame
}

/// Reset effective values without discarding explicitly reserved frame storage.
/// Construction and seek share this initializer; ordinary forward ticks do not
/// enter it. Indirect row payloads may still allocate, just as on the old reset.
fn reset_base_frame(compiled: &CompiledScene, frame: &mut FrameState, time: f64) {
    let count = compiled.objects().len();
    frame.time = time;
    frame.objects.clear();
    frame.objects.extend(
        compiled
            .objects()
            .iter()
            .enumerate()
            .map(|(index, object)| FrameObjectState {
                draw_enabled: object.draw_enabled,
                glow: object.glow.clone(),
                z_index: initial_z_index(compiled, index),
                id: object.id,
                content: object.content.clone(),
                text_bounds: object.text_bounds,
                transform: affine_base_at_time(compiled, index, object.base_transform, time),
                spatial: spatial_base_at_time(compiled, index, time),
                style: object.base_style,
                appearance: initial_channel_scalar(compiled, index, Property::Appearance, 1.0),
            }),
    );
    frame.presences.clear();
    frame.presences.extend(compiled.objects().iter().enumerate().map(|(index, object)| {
        object.live && initial_channel_bool(compiled, index, Property::Presence, true)
    }));
    frame.reveals.clear();
    frame.reveals.extend(
        (0..count).map(|index| initial_channel_scalar(compiled, index, Property::Reveal, 1.0)),
    );
    frame.morphs.clear();
    frame.morphs.extend(
        (0..count).map(|index| initial_channel_scalar(compiled, index, Property::Morph, 0.0)),
    );
    frame.render_geometries.clear();
    frame.render_geometries.resize(count, None);
    frame.render_transforms.clear();
    frame.render_transforms.resize(count, None);
    frame.family_animations.clear();
    frame.family_animations.resize(count, None);
    frame.family_animation_plan_indices.clear();
    frame.family_animation_plan_indices.resize(count, None);
}

'''
source = source[:start] + new_initializer + source[end:]
replacements = [
    ('mod frame;\n', 'mod frame;\n#[cfg(test)]\nmod seek_storage_tests;\n'),
    ('        self.frame = base_frame(&self.compiled, time);',
     '        reset_base_frame(&self.compiled, &mut self.frame, time);'),
    ('        self.painter_order = self.compiled.painter_order().to_vec();',
     '        self.painter_order.clear();\n        self.painter_order.extend_from_slice(self.compiled.painter_order());'),
]
for old, new in replacements:
    if source.count(old) != 1:
        raise SystemExit(f'nonunique repair anchor: {old!r}')
    source = source.replace(old, new, 1)
assert 'initial_bool_property(' not in source
assert 'initial_scalar_property(' not in source
PATH.write_text(source)

tests = r'''//! Seek must not invalidate a caller's explicit backing-store reservation.
use super::*;
use noon_compile::CompiledObject;
use noon_core::{CompositionTimeMap, RateFunction, TrackId};

fn object(id: u64) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::circle(0.5),
        Transform2D::IDENTITY,
        Style::default(),
    )
}

fn scene() -> CompiledScene {
    let path = VectorPath::new().move_to(Vec2::new(-1.0, 0.0)).line_to(Vec2::new(1.0, 0.0));
    let target = VectorPath::new().move_to(Vec2::new(0.0, -1.0)).line_to(Vec2::new(0.0, 1.0));
    let pair = GeometryRef::path(path.with_morph_target(target.clone()));
    let mut tracks = Vec::new();
    for (property, values, start, duration) in [
        (Property::Position, TrackValues::Vec2 { from: Vec2::new(2.0, 3.0), to: Vec2::new(5.0, -1.0) }, 1.0, 2.0),
        (Property::Appearance, TrackValues::Scalar { from: 0.2, to: 0.8 }, 1.0, 1.0),
        (Property::Appearance, TrackValues::Scalar { from: 0.8, to: 0.4 }, 2.0, 1.0),
        (Property::Reveal, TrackValues::Scalar { from: 0.3, to: 1.0 }, 1.0, 2.0),
        (Property::Presence, TrackValues::Bool { from: false, to: true }, 0.75, 0.0),
        (Property::Morph, TrackValues::PreparedMorph {
            from: 0.25, to: 0.75, geometry: pair,
            render_frame: Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY)),
            source_transform: Transform2D::IDENTITY,
        }, 1.0, 2.0),
    ] {
        tracks.push(TrackDefinition {
            id: TrackId::new(tracks.len() as u64),
            object: ObjectId::new(if property == Property::Morph { 1 } else { 0 }),
            property,
            values,
            timing: TrackTiming::new(start, duration, RateFunction::Linear),
            time_map: CompositionTimeMap::identity(),
        });
    }
    let mut compiled = CompiledScene::compile_objects(vec![
        object(0),
        CompiledObject::new(ObjectId::new(1), GeometryRef::path(target), Transform2D::IDENTITY, Style::default()),
        object(2),
    ], &tracks).unwrap();
    compiled.apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(2))).unwrap();
    compiled
}

fn storage(runtime: &SceneInstance) -> [(*const (), usize); 10] {
    macro_rules! entry {
        ($values:expr) => { ($values.as_ptr().cast::<()>(), $values.capacity()) };
    }
    [
        entry!(runtime.frame.objects),
        entry!(runtime.frame.presences),
        entry!(runtime.frame.reveals),
        entry!(runtime.frame.morphs),
        entry!(runtime.frame.render_geometries),
        entry!(runtime.frame.render_transforms),
        entry!(runtime.frame.family_animations),
        entry!(runtime.frame.family_animation_plan_indices),
        entry!(runtime.painter_ranks),
        entry!(runtime.painter_order),
    ]
}

fn reserve(runtime: &mut SceneInstance, total: usize) {
    macro_rules! reserve {
        ($values:expr) => {
            $values.try_reserve_exact(total.saturating_sub($values.len())).unwrap();
        };
    }
    reserve!(runtime.frame.objects);
    reserve!(runtime.frame.presences);
    reserve!(runtime.frame.reveals);
    reserve!(runtime.frame.morphs);
    reserve!(runtime.frame.render_geometries);
    reserve!(runtime.frame.render_transforms);
    reserve!(runtime.frame.family_animations);
    reserve!(runtime.frame.family_animation_plan_indices);
    reserve!(runtime.painter_ranks);
    reserve!(runtime.painter_order);
}

#[test]
fn reset_reuses_every_frame_buffer_and_matches_previous_initializer() {
    let mut runtime = SceneInstance::new(scene());
    reserve(&mut runtime, 12);
    let allocated = storage(&runtime);
    let empty = CompiledScene::compile_objects(Vec::new(), &[]).unwrap();
    for compiled in [&runtime.compiled, &empty, &runtime.compiled] {
        for time in [3.0, 0.0, 1.5, 0.25] {
            // Stale derived values must be removed without replacing the Vecs.
            let overlay = Arc::new(GeometryRef::circle(9.0));
            let weak = Arc::downgrade(&overlay);
            if !runtime.frame.objects.is_empty() {
                runtime.frame.render_geometries[0] = Some(overlay.clone());
                runtime.frame.render_transforms[0] = Some(Transform2D::IDENTITY);
                runtime.frame.family_animation_plan_indices[0] = Some(99);
            }
            drop(overlay);
            reset_base_frame(compiled, &mut runtime.frame, time);
            assert_eq!(runtime.frame, reference_base_frame(compiled, time));
            assert!(weak.upgrade().is_none(), "reset retained a stale render override");
            assert_eq!(storage(&runtime), allocated);
        }
    }
}

#[test]
fn ordinary_seek_evaluate_and_backward_advance_keep_reserved_storage() {
    let compiled = scene();
    let mut runtime = SceneInstance::new(compiled.clone());
    reserve(&mut runtime, 12);
    let allocated = storage(&runtime);
    for (operation, time) in [(0, 3.0), (0, 0.25), (1, 2.0), (1, 0.0), (2, 1.5), (2, 0.5)] {
        let mut reference = SceneInstance::new(compiled.clone());
        reference.advance_to(time).unwrap();
        match operation {
            0 => { runtime.seek(time).unwrap(); }
            1 => { runtime.evaluate(time).unwrap(); }
            _ => { runtime.advance_to(time).unwrap(); }
        }
        assert_eq!(runtime.frame(), reference.frame());
        assert_eq!(runtime.painter_order(), reference.painter_order());
        assert_eq!(storage(&runtime), allocated);
    }
    let frame = runtime.frame().clone();
    let publication = runtime.publication_context();
    assert!(runtime.seek(f64::NAN).is_err());
    assert_eq!(runtime.frame(), &frame);
    assert_eq!(runtime.publication_context(), publication);
    assert_eq!(storage(&runtime), allocated);
}

#[cfg(feature = "prepared-prefix")]
#[test]
fn prepared_spare_capacity_survives_rewind_then_later_suffix_and_source_disposal() {
    for terminal in [false, true] {
        let mut source = SceneInstance::new(CompiledScene::compile_objects(vec![object(0)], &[]).unwrap());
        source.begin_replay_retention(ReplayLimits { revisions: 16, payloads: 256 }).unwrap();
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        receiver.reserve_object_capacity(&mut runtime, 8).unwrap();
        let allocated = storage(&runtime);
        for time in [0.75, 0.25, 0.5] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(storage(&runtime), allocated);
        }
        source.apply_execution_patch(&ExecutionPatch::CreateObject(object(1))).unwrap();
        source.advance_to(2.0).unwrap();
        let first = source.prepare_prefix_update(&mut publisher).unwrap();
        // The reservation survives seeks: neither batch needs another barrier.
        receiver.preflight_append(&runtime, &first).unwrap();
        receiver.append(&mut runtime, first).unwrap();
        for time in [1.5, 0.5] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(storage(&runtime), allocated);
        }
        source.apply_execution_patch(&ExecutionPatch::CreateObject(object(2))).unwrap();
        source.advance_to(3.0).unwrap();
        source.apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1))).unwrap();
        source.advance_to(4.0).unwrap();
        let second = if terminal {
            source.seal_replay().unwrap();
            source.prepare_prefix_finish(&mut publisher).unwrap()
        } else {
            source.prepare_prefix_update(&mut publisher).unwrap()
        };
        let frame = runtime.frame().clone();
        let publication = runtime.publication_context();
        receiver.preflight_append(&runtime, &second).unwrap();
        receiver.append(&mut runtime, second).unwrap();
        assert_eq!(runtime.frame(), &frame);
        assert_eq!(runtime.publication_context(), publication);
        assert_eq!(storage(&runtime), allocated);
        if !terminal {
            source.seal_replay().unwrap();
            let last = source.prepare_prefix_finish(&mut publisher).unwrap();
            receiver.append(&mut runtime, last).unwrap();
        }
        drop((source, publisher));
        for (time, order) in [(2.5, vec![0, 1, 2]), (0.5, vec![0]), (3.5, vec![0, 2]), (1.5, vec![0, 1]), (4.0, vec![0, 2])] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(runtime.painter_order(), order);
            assert_eq!(runtime.frame().presences.iter().filter(|&&live| live).count(), order.len());
            assert_eq!(storage(&runtime), allocated);
        }
        assert_eq!(receiver.end_time(), Some(4.0));
    }
}

// Test-only reference: the exact previous bulk initializer, kept independent of
// the new in-place path so first-channel selection and reset defaults can differ
// only if this regression fails. It is not another production evaluator.
'''
tests += old_initializer.replace('fn base_frame(', 'fn reference_base_frame(', 1)
test_path = PATH.with_name('seek_storage_tests.rs')
if test_path.exists():
    raise SystemExit('refusing to replace an existing seek storage test module')
test_path.write_text(tests)
print('Applied shared seek repair and three regression cases; original lib blob:', actual)
