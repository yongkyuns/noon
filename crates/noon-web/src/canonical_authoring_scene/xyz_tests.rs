use super::*;
use noon::{AnimationOptions, RateFunction};
use noon_core::{SemanticVec3, VectorPath};

#[test]
fn recursive_full_coordinates_observe_detached_roots_and_effective_children() {
    let mut context = CanonicalAuthoringScene::default();
    let points = vec![
        SemanticVec3::new(0.125, 0.25, 0.123456789012345),
        SemanticVec3::new(0.5, 0.75, 0.25),
        SemanticVec3::new(1.0, 0.75, 0.75),
        SemanticVec3::new(1.5, 0.25, 1.25),
    ];
    let own = SemanticVec3::new(2.0, 3.0, 4.25);
    let child = context
        .scene
        .path(
            VectorPath::from_manim_points_3d(points.clone()),
            Default::default(),
        )
        .unwrap();
    let family = context.scene.family(&[(&child).into()]).unwrap();
    family.set_points_3d(&[own]).unwrap();
    let empty = context.scene.family(&[]).unwrap();
    let authored = [vec![own], points].concat();
    assert!(!context.identities.contains_key(&family.node_id()));
    assert_eq!(context.family_all_points_3d(&family).unwrap(), authored);
    assert_eq!(
        context.family_all_points(&family).unwrap(),
        authored
            .iter()
            .copied()
            .map(SemanticVec3::xy)
            .collect::<Vec<_>>()
    );

    let mut target = child.target_editor().unwrap();
    target.set_translation(4.0, -2.0).unwrap();
    context.bind_mobject(ObjectId::new(0), &child).unwrap();
    let animation = context
        .declare_live_transform_to(
            &child,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    {
        let player = context.live_player(2.0).unwrap();
        player.live_play_animation(&animation).unwrap();
        player.live_advance_segment_to(1.0).unwrap();
    }
    let observed = context.family_all_points_3d(&family).unwrap();
    assert_eq!(observed[0], own);
    for (actual, expected) in observed[1..].iter().zip(&authored[1..]) {
        assert!((actual.x - expected.x - 2.0).abs() < 1e-6);
        assert!((actual.y - expected.y + 1.0).abs() < 1e-6);
        assert_eq!(actual.z, expected.z);
    }
    assert_eq!(
        context.family_all_points(&family).unwrap(),
        observed
            .into_iter()
            .map(SemanticVec3::xy)
            .collect::<Vec<_>>()
    );
    assert!(context.family_all_points_3d(&empty).unwrap().is_empty());
    let foreign = noon::Scene::new().family(&[]).unwrap();
    assert!(context.family_all_points_3d(&foreign).is_err());
    let player = context.take_execution_player(2.0, 17).unwrap();
    assert!(context.family_all_points_3d(&family).is_err());
    assert!(context.family_all_points_3d(&empty).is_err());
    drop(player);
}
