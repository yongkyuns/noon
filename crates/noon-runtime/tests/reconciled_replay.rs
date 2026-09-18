use noon_compile::{CompiledObject, CompiledScene, ExecutionPatch};
use noon_core::{
    Color, CompositionTimeMap, GeometryRef, ObjectId, Property, RateFunction, Style,
    TrackDefinition, TrackId, TrackTiming, TrackValues, Transform2D, Vec2, VectorPath,
};
use noon_runtime::SceneInstance;

fn horizontal() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(-1.0, 0.0))
        .line_to(Vec2::new(1.0, 0.0))
}

fn vertical() -> VectorPath {
    VectorPath::new()
        .move_to(Vec2::new(0.0, -2.0))
        .line_to(Vec2::new(0.0, 2.0))
}

fn completed_round_trip() -> SceneInstance {
    let object = ObjectId::new(0);
    let base = CompiledObject::new(
        object,
        GeometryRef::path(horizontal()),
        Transform2D::IDENTITY,
        Style {
            fill: Some(Color::WHITE),
            ..Style::default()
        },
    );
    let mut tracks = Vec::new();
    for reverse in [false, true] {
        let (from, to, from_path, to_path, from_color, to_color) = if reverse {
            (
                4.0,
                0.0,
                vertical(),
                horizontal(),
                Color::rgb(1.0, 0.0, 0.0),
                Color::WHITE,
            )
        } else {
            (
                0.0,
                4.0,
                horizontal(),
                vertical(),
                Color::WHITE,
                Color::rgb(1.0, 0.0, 0.0),
            )
        };
        for (property, values) in [
            (
                Property::Position,
                TrackValues::Vec2 {
                    from: Vec2::new(from, 0.0),
                    to: Vec2::new(to, 0.0),
                },
            ),
            (
                Property::Fill,
                TrackValues::Color {
                    from: Some(from_color),
                    to: Some(to_color),
                },
            ),
            (
                Property::Morph,
                TrackValues::PreparedMorph {
                    from: 0.0,
                    to: 1.0,
                    geometry: GeometryRef::path(from_path.with_morph_target(to_path)),
                    render_transform: Some(Transform2D::IDENTITY),
                },
            ),
        ] {
            tracks.push(TrackDefinition {
                id: TrackId::new(tracks.len() as u64),
                object,
                property,
                values,
                timing: TrackTiming::new(
                    if reverse { 2.0 } else { 0.0 },
                    1.0,
                    RateFunction::Linear,
                ),
                time_map: CompositionTimeMap::identity(),
            });
        }
    }
    let mut runtime =
        SceneInstance::new(CompiledScene::compile_objects(vec![base], &tracks).unwrap());
    runtime.seek(3.0).unwrap();
    for track in tracks {
        runtime
            .apply_execution_patch(&ExecutionPatch::ReconcileTrack {
                track: track.id,
                object,
                property: track.property,
                end_time: track.timing.start_time + track.timing.duration,
            })
            .unwrap();
    }
    runtime
}

fn assert_intermediate_target(runtime: &SceneInstance) {
    let frame = runtime.frame();
    assert_eq!(frame.objects[0].transform.translation, Vec2::new(4.0, 0.0));
    assert_eq!(frame.objects[0].style.fill, Some(Color::rgb(1.0, 0.0, 0.0)));
    let Some(GeometryRef::VectorPath(path)) = frame.render_geometry(0) else {
        panic!("the earlier morph's target must remain visible until the next morph starts");
    };
    assert_eq!(frame.morph(0), 1.0);
    assert_eq!(
        path.morph_target().unwrap().commands(),
        vertical().commands()
    );
    assert_eq!(frame.render_transforms[0], Some(Transform2D::IDENTITY));
}

#[test]
fn reconciled_replay_preserves_completed_target_during_intermediate_hold() {
    let mut direct = completed_round_trip();
    let mut forward = completed_round_trip();
    forward.seek(0.0).unwrap();
    for time in [1.0, 1.001, 1.5, 1.999] {
        direct.seek(time).unwrap();
        forward.advance_to(time).unwrap();
        assert_intermediate_target(&direct);
        assert_intermediate_target(&forward);
        assert_eq!(direct.frame(), forward.frame());
    }
}

#[test]
fn final_reconciled_track_still_releases_to_authored_state() {
    let mut runtime = completed_round_trip();
    for time in [3.0, 3.5, 4.0] {
        runtime.seek(time).unwrap();
        assert_eq!(runtime.frame().objects[0].transform.translation, Vec2::ZERO);
        assert_eq!(runtime.frame().objects[0].style.fill, Some(Color::WHITE));
        assert_eq!(
            runtime.frame().objects[0].geometry(),
            Some(&GeometryRef::path(horizontal()))
        );
        assert_eq!(runtime.frame().morph(0), 0.0);
        assert!(runtime.frame().render_geometries[0].is_none());
    }
}
