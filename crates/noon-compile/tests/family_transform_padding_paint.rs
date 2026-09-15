use noon_compile::{
    lower_prepared_family_transform_channels, lower_prepared_semantic_animation_schedule,
    prepare_family_transform_activations, EffectiveAnimationProperties,
    PreparedFamilyTransformChannelProjection, SemanticAnimationCompletion, SemanticExecutionIndex,
};
use noon_core::{
    AnimationOptions, Color, Property, RateFunction, SemanticMutationTransaction,
    SemanticObjectState, SemanticStore, SemanticStyle, StoredGeometry, Style, TrackValues,
    Transform2D,
};

fn style(alpha: f32) -> Style {
    Style {
        fill: Some(Color::rgba(1.0, 1.0, 1.0, alpha)),
        stroke: Some(Color::rgba(1.0, 0.0, 0.0, alpha)),
        stroke_width: 0.0,
        ..Style::default()
    }
}

fn lower(
    source_alphas: &[f32],
    target_alphas: &[f32],
    appearance: f32,
) -> PreparedFamilyTransformChannelProjection {
    let mut store = SemanticStore::new();
    let source = store.insert_family();
    let target = store.insert_family();
    let mut sources = Vec::new();
    for (family, alphas) in [(source, source_alphas), (target, target_alphas)] {
        for &alpha in alphas {
            let mut object = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
            object.style = SemanticStyle::from_compact(style(alpha));
            let id = store.insert_semantic_object(object);
            store.add_member(family, id).unwrap();
            if family == source {
                sources.push(id);
            }
        }
    }
    store.attach_to_scene(source).unwrap();
    let mut index = SemanticExecutionIndex::new();
    index.lower_scene(&store).unwrap();
    let source_ids = sources
        .iter()
        .map(|&id| index.execution_object_id(id).unwrap())
        .collect::<Vec<_>>();
    let mut transaction = SemanticMutationTransaction::new();
    let animation = transaction.create_family_transform_animation(
        source,
        target,
        AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear),
    );
    let prepared = transaction.prepare(&mut store).unwrap();
    let schedule = lower_prepared_semantic_animation_schedule(
        &prepared,
        &index,
        animation,
        0.0,
        AnimationOptions::new(),
    )
    .unwrap();
    let activation = prepare_family_transform_activations(&prepared, &index, &schedule, |id| {
        let position = source_ids.iter().position(|source| *source == id).unwrap();
        Some(EffectiveAnimationProperties {
            z_index: 0.0,
            transform: Transform2D::IDENTITY,
            style: style(source_alphas[position]),
            appearance,
            reveal: 1.0,
        })
    })
    .unwrap();
    lower_prepared_family_transform_channels(&prepared, &activation).unwrap()
}

fn alpha(values: &TrackValues, progress: f32) -> f32 {
    let TrackValues::Color { from, to } = values else {
        panic!("expected color")
    };
    let a = from.map_or(0.0, |color| color.alpha);
    let b = to.map_or(0.0, |color| color.alpha);
    a * (1.0 - progress) + b * progress
}

#[test]
fn transparent_source_to_padding_never_acquires_ghost_fill_or_stroke() {
    // Manim's 3->2 family alignment is [t0, transparent_copy(t0), t1].
    // Both visible endpoints of the middle occurrence are transparent.
    let result = lower(&[1.0, 0.0, 1.0], &[1.0, 1.0], 1.0);
    let hidden = result
        .stable_tracks()
        .iter()
        .find(|row| row.retain_effective)
        .unwrap();
    for property in [Property::Fill, Property::Stroke] {
        let row = result
            .stable_tracks()
            .iter()
            .find(|row| {
                row.track.execution_object_id == hidden.track.execution_object_id
                    && row.track.property == property
            })
            .unwrap();
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(alpha(&row.track.values, t) * (1.0 - t), 0.0);
        }
        // Presentation alpha normalization must not change authored reconciliation.
        assert!(matches!(
            row.track.completion,
            SemanticAnimationCompletion::Fill { opacity: 1.0, .. }
                | SemanticAnimationCompletion::Stroke { opacity: 1.0, .. }
        ));
    }
}

#[test]
fn source_padding_grows_with_one_fade_not_a_squared_fade() {
    let result = lower(&[0.0, 1.0], &[1.0, 1.0, 1.0], 1.0);
    let copy = &result.derived_occurrences()[0];
    for property in [Property::Fill, Property::Stroke] {
        let row = copy
            .tracks
            .iter()
            .find(|row| row.property == property)
            .unwrap();
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(alpha(&row.values, t) * t, t);
        }
    }
}

#[test]
fn hidden_real_source_returning_to_transparent_paint_stays_transparent() {
    let result = lower(&[1.0], &[0.0], 0.0);
    for property in [Property::Fill, Property::Stroke] {
        let row = result
            .stable_tracks()
            .iter()
            .find(|row| row.track.property == property)
            .unwrap();
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(alpha(&row.track.values, t) * t, 0.0);
        }
    }
    assert!(result.stable_tracks().iter().any(|row| {
        row.track.property == Property::Appearance
            && !row.retain_effective
            && row.track.values == TrackValues::Scalar { from: 0.0, to: 1.0 }
    }));
}

#[test]
fn ordinary_unpadded_color_interpolation_is_unchanged() {
    let result = lower(&[0.2], &[0.8], 1.0);
    for property in [Property::Fill, Property::Stroke] {
        let row = result
            .stable_tracks()
            .iter()
            .find(|row| row.track.property == property)
            .unwrap();
        assert!((alpha(&row.track.values, 0.5) - 0.5).abs() < 1e-6);
    }
    assert!(result
        .stable_tracks()
        .iter()
        .all(|row| row.track.property != Property::Appearance));
}
