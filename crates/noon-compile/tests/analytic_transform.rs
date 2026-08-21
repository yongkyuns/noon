use noon_compile::{CompiledScene, TransformGeometryPlan};
use noon_core::{
    Easing, GeometryRef, ObjectSnapshot, SceneDefinition, TrackTiming, Transform2D, Vec2,
};

fn add_transform(scene: &mut SceneDefinition, from: GeometryRef, to: GeometryRef) {
    let object = scene.add(from.clone());
    scene
        .animate_transform(
            object,
            ObjectSnapshot {
                geometry: from,
                transform: Transform2D::IDENTITY,
                style: scene.object(object).unwrap().style,
            },
            ObjectSnapshot {
                geometry: to,
                transform: Transform2D::IDENTITY,
                style: scene.object(object).unwrap().style,
            },
            TrackTiming::new(0.0, 2.0, Easing::Linear),
        )
        .unwrap();
}

#[test]
fn compiler_selects_analytic_geometry_plans() {
    let mut scene = SceneDefinition::new();
    add_transform(
        &mut scene,
        GeometryRef::circle(1.0),
        GeometryRef::circle(3.0),
    );
    add_transform(
        &mut scene,
        GeometryRef::rectangle(2.0, 4.0),
        GeometryRef::rectangle(6.0, 8.0),
    );
    add_transform(
        &mut scene,
        GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        GeometryRef::line(Vec2::new(0.0, -2.0), Vec2::new(0.0, 2.0)),
    );

    let compiled = CompiledScene::compile(&scene).unwrap();
    assert!(matches!(
        compiled.tracks()[0].transform_geometry_plan,
        Some(TransformGeometryPlan::Circle {
            from_radius: 1.0,
            to_radius: 3.0
        })
    ));
    assert!(matches!(
        compiled.tracks()[1].transform_geometry_plan,
        Some(TransformGeometryPlan::Rectangle {
            from_size: Vec2 { x: 2.0, y: 4.0 },
            to_size: Vec2 { x: 6.0, y: 8.0 }
        })
    ));
    assert!(matches!(
        compiled.tracks()[2].transform_geometry_plan,
        Some(TransformGeometryPlan::Line {
            from_start: Vec2 { x: -1.0, y: 0.0 },
            from_end: Vec2 { x: 1.0, y: 0.0 },
            to_start: Vec2 { x: 0.0, y: -2.0 },
            to_end: Vec2 { x: 0.0, y: 2.0 }
        })
    ));
}
