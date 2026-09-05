use noon_compile::{CompiledObject, CompiledScene};
use noon_core::{
    CompositionTimeMap, CompositionTimeMapStep, GeometryRef, ObjectId, Property, RateFunction,
    RetainedObjectDefinition, SceneDefinition, TextResourceHandle, TextResourceId, TimelineError,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Vec2,
};
use noon_runtime::SceneInstance;

fn text_handle() -> TextResourceHandle {
    TextResourceHandle {
        arena: 0,
        id: TextResourceId::new(41),
        version: 7,
    }
}

fn track(
    id: u64,
    object: ObjectId,
    property: Property,
    values: TrackValues,
    start_time: f64,
    duration: f64,
) -> TrackDefinition {
    TrackDefinition {
        id: TrackId::new(id),
        object,
        property,
        values,
        timing: TrackTiming::new(start_time, duration, RateFunction::Linear),
        time_map: CompositionTimeMap::identity(),
    }
}

#[test]
fn ordinary_properties_accept_exact_instant_assignments() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));

    scene
        .add_track(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::new(4.0, -2.0),
                to: Vec2::ZERO,
            },
            TrackTiming::instant(1.0),
        )
        .expect("ordinary property assignments must support an exact timestamp");

    let negative = scene
        .add_track(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::ONE,
            },
            TrackTiming::new(2.0, -1.0, RateFunction::Linear),
        )
        .expect_err("negative-duration assignments must remain invalid");
    assert!(matches!(negative, TimelineError::InvalidDuration(value) if value == -1.0));

    let mapped_instant = scene
        .add_track_with_time_map(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::new(0.5, 0.5),
                to: Vec2::ONE,
            },
            TrackTiming::instant(2.0),
            CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.0,
                1.0,
                RateFunction::Linear,
            )]),
        )
        .expect_err("instant assignments cannot carry a composition time map");
    assert!(matches!(
        mapped_instant,
        TimelineError::InstantTrackCannotUseTimeMap(Property::Scale)
    ));

    let positive_presence = scene
        .add_track(
            object,
            Property::Presence,
            TrackValues::Bool {
                from: true,
                to: false,
            },
            TrackTiming::new(3.0, 0.5, RateFunction::Linear),
        )
        .expect_err("Presence remains a discrete zero-duration channel");
    assert!(matches!(
        positive_presence,
        TimelineError::InvalidInstantDuration {
            property: Property::Presence,
            duration: 0.5,
        }
    ));
}

#[test]
fn legacy_cleanup_assignments_restore_canonical_state_at_the_hide_boundary() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));
    let faded_position = Vec2::new(2.0, -1.0);
    let faded_scale = Vec2::new(0.5, 0.5);

    scene
        .animate_position(
            object,
            Vec2::ZERO,
            faded_position,
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        )
        .unwrap();
    scene
        .animate_scale(
            object,
            Vec2::ONE,
            faded_scale,
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        )
        .unwrap();
    scene
        .animate_appearance(
            object,
            1.0,
            0.0,
            TrackTiming::new(0.0, 1.0, RateFunction::Linear),
        )
        .unwrap();

    scene
        .add_track(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: faded_position,
                to: Vec2::ZERO,
            },
            TrackTiming::instant(1.0),
        )
        .unwrap();
    scene
        .add_track(
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: faded_scale,
                to: Vec2::ONE,
            },
            TrackTiming::instant(1.0),
        )
        .unwrap();
    scene
        .add_track(
            object,
            Property::Appearance,
            TrackValues::Scalar { from: 0.0, to: 1.0 },
            TrackTiming::instant(1.0),
        )
        .unwrap();
    scene.set_presence_at(object, true, false, 1.0).unwrap();
    scene.set_presence_at(object, false, true, 2.0).unwrap();

    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut direct = SceneInstance::new(compiled.clone());
    let mut sequential = SceneInstance::new(compiled);

    let before = direct.seek(0.999).unwrap();
    assert!(before.objects[0].transform.translation.x > 1.99);
    assert!(before.objects[0].transform.scale.x < 0.501);
    assert!(before.appearance(0) < 0.0011);
    assert!(before.is_present(0));

    let hidden = direct.seek(1.0).unwrap();
    assert!(!hidden.is_present(0));
    assert_eq!(hidden.objects[0].transform.translation, Vec2::ZERO);
    assert_eq!(hidden.objects[0].transform.scale, Vec2::ONE);
    assert_eq!(hidden.appearance(0), 1.0);

    let shown = direct.seek(2.0).unwrap();
    assert!(shown.is_present(0));
    assert_eq!(shown.objects[0].transform.translation, Vec2::ZERO);
    assert_eq!(shown.objects[0].transform.scale, Vec2::ONE);
    assert_eq!(shown.appearance(0), 1.0);

    for step in 1..=20 {
        sequential.advance_to(f64::from(step) * 0.1).unwrap();
    }
    assert_eq!(sequential.frame(), direct.frame());
}

#[test]
fn retained_text_cleanup_assignments_are_seekable_and_preserve_resource_identity() {
    let object = ObjectId::new(3);
    let handle = text_handle();
    let objects = [RetainedObjectDefinition::text(object, handle)];
    let faded_position = Vec2::new(-3.0, 1.5);
    let faded_scale = Vec2::new(0.25, 0.25);
    let tracks = [
        track(
            0,
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: faded_position,
            },
            0.0,
            1.0,
        ),
        track(
            1,
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: Vec2::ONE,
                to: faded_scale,
            },
            0.0,
            1.0,
        ),
        track(
            2,
            object,
            Property::Appearance,
            TrackValues::Scalar { from: 1.0, to: 0.0 },
            0.0,
            1.0,
        ),
        track(
            3,
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: faded_position,
                to: Vec2::ZERO,
            },
            1.0,
            0.0,
        ),
        track(
            4,
            object,
            Property::Scale,
            TrackValues::Vec2 {
                from: faded_scale,
                to: Vec2::ONE,
            },
            1.0,
            0.0,
        ),
        track(
            5,
            object,
            Property::Appearance,
            TrackValues::Scalar { from: 0.0, to: 1.0 },
            1.0,
            0.0,
        ),
        track(
            6,
            object,
            Property::Presence,
            TrackValues::Bool {
                from: true,
                to: false,
            },
            1.0,
            0.0,
        ),
        track(
            7,
            object,
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            2.0,
            0.0,
        ),
    ];

    let objects = objects
        .iter()
        .map(|object| {
            CompiledObject::new(
                object.id,
                object.content.clone(),
                object.transform,
                object.style,
            )
        })
        .collect();
    let compiled = CompiledScene::compile_objects(objects, &tracks).unwrap();
    let mut direct = SceneInstance::new(compiled.clone());
    let mut sequential = SceneInstance::new(compiled);

    let hidden = direct.seek(1.0).unwrap();
    assert!(!hidden.is_present(0));
    assert_eq!(hidden.objects[0].transform.translation, Vec2::ZERO);
    assert_eq!(hidden.objects[0].transform.scale, Vec2::ONE);
    assert_eq!(hidden.appearance(0), 1.0);
    assert_eq!(hidden.text(0), Some(handle));
    assert_eq!(hidden.render_geometry(0), None);

    let shown = direct.seek(2.0).unwrap();
    assert!(shown.is_present(0));
    assert_eq!(shown.objects[0].transform.translation, Vec2::ZERO);
    assert_eq!(shown.objects[0].transform.scale, Vec2::ONE);
    assert_eq!(shown.appearance(0), 1.0);
    assert_eq!(shown.text(0), Some(handle));

    for step in 1..=20 {
        sequential.advance_to(f64::from(step) * 0.1).unwrap();
    }
    assert_eq!(sequential.frame(), direct.frame());
}

#[test]
fn instant_assignment_wins_in_a_channel_that_also_has_a_composition_map() {
    let mut scene = SceneDefinition::new();
    let object = scene.add(GeometryRef::circle(1.0));

    scene
        .add_track_with_time_map(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::new(4.0, 0.0),
            },
            TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.0,
                1.0,
                RateFunction::Smooth,
            )]),
        )
        .unwrap();
    scene
        .add_track(
            object,
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::new(4.0, 0.0),
                to: Vec2::ZERO,
            },
            TrackTiming::instant(2.0),
        )
        .unwrap();

    let compiled = CompiledScene::compile(&scene).unwrap();
    let mut direct = SceneInstance::new(compiled.clone());
    let mut sequential = SceneInstance::new(compiled);

    assert_eq!(
        direct.seek(2.0).unwrap().objects[0].transform.translation,
        Vec2::ZERO
    );
    for step in 1..=20 {
        sequential.advance_to(f64::from(step) * 0.1).unwrap();
    }
    assert_eq!(sequential.frame(), direct.frame());
}
