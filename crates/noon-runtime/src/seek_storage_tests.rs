//! Seek must not invalidate a caller's explicit backing-store reservation.
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
    let path = VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0));
    let target = VectorPath::new()
        .move_to(Vec2::new(0.0, -1.0))
        .line_to(Vec2::new(0.0, 1.0));
    let pair = GeometryRef::path(path.with_morph_target(target.clone()));
    let mut tracks = Vec::new();
    for (property, values, start, duration) in [
        (
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::new(2.0, 3.0),
                to: Vec2::new(5.0, -1.0),
            },
            1.0,
            2.0,
        ),
        (
            Property::Appearance,
            TrackValues::Scalar { from: 0.2, to: 0.8 },
            1.0,
            1.0,
        ),
        (
            Property::Appearance,
            TrackValues::Scalar { from: 0.8, to: 0.4 },
            2.0,
            1.0,
        ),
        (
            Property::Reveal,
            TrackValues::Scalar { from: 0.3, to: 1.0 },
            1.0,
            2.0,
        ),
        (
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            0.75,
            0.0,
        ),
        (
            Property::Morph,
            TrackValues::PreparedMorph {
                from: 0.25,
                to: 0.75,
                geometry: pair,
                render_frame: Some(noon_core::MorphRenderFrame::fixed(Transform2D::IDENTITY)),
                source_transform: Transform2D::IDENTITY,
            },
            1.0,
            2.0,
        ),
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
    let mut compiled = CompiledScene::compile_objects(
        vec![
            object(0),
            CompiledObject::new(
                ObjectId::new(1),
                GeometryRef::path(target),
                Transform2D::IDENTITY,
                Style::default(),
            ),
            object(2),
        ],
        &tracks,
    )
    .unwrap();
    compiled
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(2)))
        .unwrap();
    compiled
}

fn storage(runtime: &SceneInstance) -> [(*const (), usize); 10] {
    macro_rules! entry {
        ($values:expr) => {
            ($values.as_ptr().cast::<()>(), $values.capacity())
        };
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
            $values
                .try_reserve_exact(total.saturating_sub($values.len()))
                .unwrap();
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
            assert!(
                weak.upgrade().is_none(),
                "reset retained a stale render override"
            );
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
            0 => {
                runtime.seek(time).unwrap();
            }
            1 => {
                runtime.evaluate(time).unwrap();
            }
            _ => {
                runtime.advance_to(time).unwrap();
            }
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
        let mut source =
            SceneInstance::new(CompiledScene::compile_objects(vec![object(0)], &[]).unwrap());
        source
            .begin_replay_retention(ReplayLimits {
                revisions: 16,
                payloads: 256,
            })
            .unwrap();
        source.advance_to(1.0).unwrap();
        let (mut publisher, mut runtime, mut receiver) = source.create_prefix_projection().unwrap();
        receiver.reserve_object_capacity(&mut runtime, 8).unwrap();
        let allocated = storage(&runtime);
        for time in [0.75, 0.25, 0.5] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(storage(&runtime), allocated);
        }
        source
            .apply_execution_patch(&ExecutionPatch::CreateObject(object(1)))
            .unwrap();
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
        source
            .apply_execution_patch(&ExecutionPatch::CreateObject(object(2)))
            .unwrap();
        source.advance_to(3.0).unwrap();
        source
            .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(1)))
            .unwrap();
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
        for (time, order) in [
            (2.5, vec![0, 1, 2]),
            (0.5, vec![0]),
            (3.5, vec![0, 2]),
            (1.5, vec![0, 1]),
            (4.0, vec![0, 2]),
        ] {
            receiver.prepare_seek(&mut runtime, time).unwrap();
            runtime.seek(time).unwrap();
            assert_eq!(runtime.painter_order(), order);
            assert_eq!(
                runtime
                    .frame()
                    .presences
                    .iter()
                    .filter(|&&live| live)
                    .count(),
                order.len()
            );
            assert_eq!(storage(&runtime), allocated);
        }
        assert_eq!(receiver.end_time(), Some(4.0));
    }
}

// Test-only reference: the exact previous bulk initializer, kept independent of
// the new in-place path so first-channel selection and reset defaults can differ
// only if this regression fails. It is not another production evaluator.
fn reference_base_frame(compiled: &CompiledScene, time: f64) -> FrameState {
    let appearances = initial_scalar_property(
        compiled,
        compiled.objects().len(),
        Property::Appearance,
        1.0,
    );
    let objects: Vec<_> = compiled
        .objects()
        .iter()
        .enumerate()
        .map(|(index, object)| FrameObjectState {
            draw_enabled: object.draw_enabled,
            fill_paint: object.fill_paint.clone(),
            effects: object.effects.clone(),
            z_index: initial_z_index(compiled, index),
            id: object.id,
            content: object.content.clone(),
            text_bounds: object.text_bounds,
            transform: affine_base_at_time(compiled, index, object.base_transform, time),
            spatial: spatial_base_at_time(compiled, index, time),
            style: object.base_style,
            appearance: appearances[index],
        })
        .collect();
    let mut presences = initial_bool_property(compiled, objects.len(), Property::Presence, true);
    for (index, object) in compiled.objects().iter().enumerate() {
        if !object.live {
            presences[index] = false;
        }
    }
    FrameState {
        time,
        presences,
        reveals: initial_scalar_property(compiled, objects.len(), Property::Reveal, 1.0),
        morphs: initial_scalar_property(compiled, objects.len(), Property::Morph, 0.0),
        render_geometries: vec![None; objects.len()],
        render_transforms: vec![None; objects.len()],
        family_animations: vec![None; objects.len()],
        family_animation_plan_indices: vec![None; objects.len()],
        objects,
    }
}

fn initial_bool_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: bool,
) -> Vec<bool> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let TrackValues::Bool { from, .. } = &track.values else {
            unreachable!("compiled bool property must contain bool values");
        };
        values[index] = *from;
        initialized[index] = true;
    }
    values
}

fn initial_scalar_property(
    compiled: &CompiledScene,
    object_count: usize,
    property: Property,
    default: f32,
) -> Vec<f32> {
    let mut values = vec![default; object_count];
    let mut initialized = vec![false; object_count];
    for track in compiled
        .tracks_iter()
        .filter(|track| track.property == property)
    {
        let index = track.object_index as usize;
        if initialized[index] {
            continue;
        }
        let (TrackValues::Scalar { from, .. } | TrackValues::PreparedMorph { from, .. }) =
            &track.values
        else {
            unreachable!("compiled scalar property must contain scalar values");
        };
        values[index] = from.clamp(0.0, 1.0);
        initialized[index] = true;
    }
    values
}

#[test]
fn merged_seek_preserves_paint_effect_and_reserved_storage() {
    use noon_compile::{CompiledEffect, CompiledEffectStack};
    use noon_core::{
        EffectDefinition, EffectUpdate, Glow, GradientStop, LinearGradient, SemanticNodeId,
        SemanticPaint,
    };

    let paint = Arc::new(SemanticPaint::LinearGradient(Arc::new(
        LinearGradient::new(
            [0.0, 0.5],
            [1.0, 0.5],
            [
                GradientStop::new(0.0, Color::RED).unwrap(),
                GradientStop::new(1.0, Color::BLUE).unwrap(),
            ],
        )
        .unwrap(),
    )));
    let identities = [SemanticNodeId::new(7, 1), SemanticNodeId::new(8, 2)];
    let definitions = [
        EffectDefinition::Glow(
            Glow::new(EffectUpdate::default().color(Color::RED).intensity(0.25)).unwrap(),
        ),
        EffectDefinition::Glow(
            Glow::new(EffectUpdate::default().color(Color::BLUE).intensity(0.75)).unwrap(),
        ),
    ];
    let effect = Arc::new(
        CompiledEffectStack::new([
            CompiledEffect {
                attachment: identities[0],
                definition: definitions[0],
            },
            CompiledEffect {
                attachment: identities[1],
                definition: definitions[1],
            },
        ])
        .unwrap(),
    );
    let mut row = object(0);
    row.fill_paint = Some(paint.clone());
    row.effects = Some(effect.clone());
    let compiled = CompiledScene::compile_objects(vec![row], &[]).unwrap();
    let mut runtime = SceneInstance::new(compiled.clone());
    reserve(&mut runtime, 12);
    let allocated = storage(&runtime);
    for time in [2.0, 0.0, 0.5, 1.0, 0.0] {
        runtime.seek(time).unwrap();
        assert!(
            runtime.frame().objects[0]
                .fill_paint
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &paint)),
            "seek lost the retained paint handle"
        );
        if let Some(stack) = runtime.frame().objects[0].effects.as_ref() {
            assert_eq!(
                stack.attachments(),
                &identities,
                "seek reordered retained effect bindings"
            );
            assert_eq!(
                stack.definitions(),
                &definitions,
                "seek changed retained effect definitions"
            );
        }
        assert!(
            runtime.frame().objects[0]
                .effects
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &effect)),
            "seek lost the retained effect handle"
        );
        assert_eq!(runtime.frame(), &reference_base_frame(&compiled, time));
        assert_eq!(storage(&runtime), allocated);
    }
}
