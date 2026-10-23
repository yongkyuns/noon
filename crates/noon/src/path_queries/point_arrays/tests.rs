use super::*;
use crate::{LiveSession, Scene};
use noon_core::PointArrayShapeError;

#[test]
fn empty_point_array_shape_survives_resource_codec_and_rejects_invalid_metadata() {
    for dimensions in [vec![0], vec![0, 2], vec![0, 3], vec![0, 2, 3]] {
        let path =
            VectorPath::from_empty_point_array(PointArrayShape::new(dimensions).unwrap()).unwrap();
        let mut encoded = serde_json::to_value(&path).unwrap();
        let decoded: VectorPath = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded, path);
        encoded["empty_point_array_shape"] = serde_json::json!([]);
        assert!(serde_json::from_value::<VectorPath>(encoded.clone()).is_err());
        encoded["empty_point_array_shape"] = serde_json::json!([1, 3]);
        let invalid: VectorPath = serde_json::from_value(encoded).unwrap();
        assert!(!invalid.is_finite());
    }
}

#[test]
fn empty_point_array_shapes_survive_copy_and_promotion_but_clear_resets_them() {
    for family in [false, true] {
        for dimensions in [vec![0], vec![0, 2], vec![0, 3], vec![0, 2, 3]] {
            let mut scene = Scene::new();
            let source = if family {
                crate::CompositeEntryHandle::Family(scene.family(&[]).unwrap())
            } else {
                crate::CompositeEntryHandle::Mobject(scene.circle(0.2).unwrap())
            };
            let shape = PointArrayShape::new(dimensions).unwrap();
            source
                .as_target()
                .set_empty_point_array(shape.clone())
                .unwrap();
            let before = source.as_target().point_array().unwrap();
            assert_eq!(before.shape(), &shape);
            assert!(before.points().is_empty());
            let copied = source.as_target().promote_own_points().unwrap();
            assert_eq!(copied.root().as_target().point_array().unwrap(), before);
            // Promotion clears the original root to canonical XYZ, but its
            // copied zero-row array still participates in recursive merging.
            if shape == PointArrayShape::default() {
                assert_eq!(source.as_target().point_array().unwrap(), before);
            } else {
                assert!(matches!(
                    source.as_target().point_array(),
                    Err(AuthoringError::PointArrayShape(_))
                ));
            }
            match copied.root() {
                crate::CompositeEntryHandle::Mobject(object) => {
                    object.clone().clear_points().unwrap()
                }
                crate::CompositeEntryHandle::Family(family) => family.clear_points().unwrap(),
            }
            assert_eq!(
                source.as_target().point_array().unwrap().shape(),
                &PointArrayShape::default()
            );
        }
    }
}

#[test]
fn empty_point_array_rank_and_axis_errors_follow_recursive_append_order() {
    let mut scene = Scene::new();
    let a = scene.family(&[]).unwrap();
    a.set_empty_point_array(PointArrayShape::new(vec![0]).unwrap())
        .unwrap();
    let b = scene.family(&[]).unwrap();
    b.set_empty_point_array(PointArrayShape::new(vec![0, 2]).unwrap())
        .unwrap();
    let nested = scene.family(&[(&a).into()]).unwrap();
    nested
        .set_empty_point_array(PointArrayShape::new(vec![0, 2, 3]).unwrap())
        .unwrap();
    let root = scene.family(&[(&nested).into(), (&b).into()]).unwrap();
    assert!(matches!(
        MobjectTarget::from(&root).point_array(),
        Err(AuthoringError::PointArrayShape(
            PointArrayShapeError::RankMismatch { left: 3, right: 1 }
        ))
    ));
    a.set_empty_point_array(PointArrayShape::new(vec![0, 2, 3]).unwrap())
        .unwrap();
    assert!(matches!(
        MobjectTarget::from(&root).point_array(),
        Err(AuthoringError::PointArrayShape(
            PointArrayShapeError::RankMismatch { left: 2, right: 3 }
        ))
    ));
    nested
        .set_empty_point_array(PointArrayShape::default())
        .unwrap();
    a.set_empty_point_array(PointArrayShape::default()).unwrap();
    assert!(matches!(
        MobjectTarget::from(&root).point_array(),
        Err(AuthoringError::PointArrayShape(
            PointArrayShapeError::AxisMismatch {
                axis: 1,
                left: 3,
                right: 2
            }
        ))
    ));
}

#[test]
fn live_empty_point_array_publication_preserves_shapes_and_invalid_edits_are_atomic() {
    for family in [false, true] {
        let mut scene = Scene::new();
        let source = if family {
            crate::CompositeEntryHandle::Family(scene.family(&[]).unwrap())
        } else {
            crate::CompositeEntryHandle::Mobject(scene.circle(0.2).unwrap())
        };
        scene.add_many(&[source.as_target()]).unwrap();
        let mut execution = scene.execution_session().unwrap();
        let shape = PointArrayShape::new(vec![0, 2, 3]).unwrap();
        {
            let mut live =
                LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
            live.set_empty_point_array(source.as_target(), shape.clone())
                .unwrap();
            assert_eq!(
                live.point_array(source.as_target()).unwrap().shape(),
                &shape
            );
            let copied = live.promote_own_points(source.as_target(), &[]).unwrap();
            assert_eq!(
                live.point_array(copied.root().as_target()).unwrap().shape(),
                &shape
            );
            assert!(live.point_array(source.as_target()).is_err());
        }
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let mut live = LiveSession::new(scene.integration_store(), scene.root(), &mut execution);
        assert!(live
            .set_empty_point_array(source.as_target(), PointArrayShape::xyz(1).unwrap())
            .is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
        let foreign = Scene::new().family(&[]).unwrap();
        assert!(live
            .set_empty_point_array((&foreign).into(), shape)
            .is_err());
        assert!(live.point_array((&foreign).into()).is_err());
        assert_eq!(scene.revision(), revision);
    }
}

#[test]
fn point_array_preserves_xyz_alias_runs_and_queries_deep_families_without_recursion() {
    let mut scene = Scene::new();
    let point = SemanticVec3::new(1.125, -3.5, 9.25);
    let child = scene
        .path(
            VectorPath::from_manim_points_3d(vec![point]),
            Default::default(),
        )
        .unwrap();
    let a = scene.family(&[(&child).into()]).unwrap();
    let b = scene.family(&[(&child).into()]).unwrap();
    let mut root = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    for _ in 0..256 {
        root = scene.family(&[(&root).into()]).unwrap();
    }
    let observation = MobjectTarget::from(&root).point_array().unwrap();
    assert_eq!(observation.points(), &[point, point]);
    assert_eq!(observation.shape().dimensions(), &[2, 3]);
}
