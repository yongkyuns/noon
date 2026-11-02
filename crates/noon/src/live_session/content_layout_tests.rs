use super::*;
use crate::{LayoutAnchor, LayoutDimension, Scene};
use noon_core::{
    GeometryRef, GeometryResourceArena, GeometryResourceLookup, ObjectContentRef, VectorPath,
};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
}

#[test]
fn retained_content_layout_uses_current_controls_anchors_and_depth_until_retirement() {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.1).unwrap();
    let center = (10.123_456_789_012_345, -7.25);
    object
        .shift_3d(SemanticVec3::new(center.0, center.1, 7.0))
        .unwrap();
    object.set_scale(-2.0, 3.0).unwrap();
    object.set_rotation(std::f64::consts::FRAC_PI_2).unwrap();
    object.set_stroke_width(100.0).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    for index in 0..32 {
        let mut unrelated = scene.circle(0.2).unwrap();
        unrelated
            .set_translation(100.0 + f64::from(index), 0.0)
            .unwrap();
        scene.add(&unrelated).unwrap();
    }
    let authored = object.state().unwrap();
    let nodes = scene.integration_store().borrow().len();
    let mut execution = scene.execution_session().unwrap();
    let original = scene
        .live(&mut execution)
        .effective_layout(&object)
        .unwrap();
    let initial = execution.publication_context();
    let anchor = LayoutAnchor::from(&object);
    let family_anchor = LayoutAnchor::from(&family);

    let mut source = GeometryResourceArena::new();
    let handle = source.insert_path(VectorPath::from_manim_points_3d(vec![
        SemanticVec3::new(-1.0, 0.0, 1.0),
        SemanticVec3::new(-4.0, 3.0, 10.0),
        SemanticVec3::new(2.0, 5.0, -6.0),
        SemanticVec3::new(1.0, 0.0, 3.0),
    ]));
    let prepared = execution
        .prepare_effective_geometry_replacement(object.node_id(), handle, &source, None)
        .unwrap();
    let lease = execution
        .commit_effective_content_replacement(prepared)
        .unwrap();
    drop(source);
    execution.take_frame_changes();
    let published = execution.publication_context();
    {
        let live = scene.live(&mut execution);
        let layout = live.effective_layout(&object).unwrap();
        close(layout.center.0, center.0);
        close(layout.center.1, center.1);
        close(layout.width, 15.0);
        close(layout.height, 12.0);
        assert_eq!(live.effective_family_layout(&family).unwrap(), layout);
        close(
            live.dimension_length(&anchor, LayoutDimension::Width)
                .unwrap(),
            15.0,
        );
        close(
            live.dimension_length(&family_anchor, LayoutDimension::Height)
                .unwrap(),
            12.0,
        );
        for target in [&anchor, &family_anchor] {
            // The current path's anchors have world Z 8 and 10. Its controls
            // reach world Z 1 and 17; neither query may use the authored circle.
            close(
                live.layout_critical_point_3d(target, SemanticVec3::ZERO)
                    .unwrap()
                    .z,
                9.0,
            );
            close(
                live.layout_critical_point_3d(target, SemanticVec3::new(0.0, 0.0, -1.0))
                    .unwrap()
                    .z,
                8.0,
            );
            close(
                live.layout_critical_point_3d(target, SemanticVec3::new(0.0, 0.0, 1.0))
                    .unwrap()
                    .z,
                10.0,
            );
            close(
                live.dimension_length(target, LayoutDimension::Depth)
                    .unwrap(),
                16.0,
            );
        }
    }
    assert_eq!(execution.publication_context(), published);
    assert!(execution.take_frame_changes().is_empty());
    assert_eq!(object.state().unwrap(), authored);
    assert_eq!(scene.integration_store().borrow().len(), nodes);

    let prepared = execution
        .prepare_effective_content_replacement(
            object.node_id(),
            ObjectContentRef::Geometry(GeometryRef::Circle { radius: 2.0 }),
            None,
            Some(lease),
        )
        .unwrap();
    let lease = execution
        .commit_effective_content_replacement(prepared)
        .unwrap();
    assert!(execution.geometry_resources().get(handle).is_none());
    {
        let live = scene.live(&mut execution);
        let layout = live.effective_layout(&object).unwrap();
        close(layout.center.0, center.0);
        close(layout.center.1, center.1);
        close(layout.width, 12.0);
        close(layout.height, 8.0);
        close(
            live.layout_critical_point_3d(&anchor, SemanticVec3::ZERO)
                .unwrap()
                .z,
            7.0,
        );
        close(
            live.dimension_length(&anchor, LayoutDimension::Depth)
                .unwrap(),
            0.0,
        );
        assert_eq!(live.effective_family_layout(&family).unwrap(), layout);
    }

    execution.release_effective_content(lease).unwrap();
    {
        let live = scene.live(&mut execution);
        let restored = live.effective_layout(&object).unwrap();
        assert_eq!(restored.center, original.center);
        assert_eq!(
            (restored.width, restored.height),
            (original.width, original.height)
        );
        close(
            live.layout_critical_point_3d(&anchor, SemanticVec3::ZERO)
                .unwrap()
                .z,
            7.0,
        );
        close(
            live.dimension_length(&anchor, LayoutDimension::Depth)
                .unwrap(),
            0.0,
        );
    }
    assert_eq!(
        execution.publication_context().scene_revision(),
        initial.scene_revision()
    );
    assert_eq!(
        execution.publication_context().execution_revision(),
        initial.execution_revision()
    );
    assert_eq!(object.state().unwrap(), authored);
    assert_eq!(scene.integration_store().borrow().len(), nodes);
}

#[test]
fn empty_retained_path_keeps_leaf_family_and_anchor_layout_queries_defined() {
    let mut scene = Scene::new();
    let mut object = scene.circle(0.1).unwrap();
    object.set_translation(4.0, 5.0).unwrap();
    let family = scene.family(&[(&object).into()]).unwrap();
    scene.add_many(&[(&family).into()]).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let mut source = GeometryResourceArena::new();
    let handle = source.insert_path(VectorPath::new());
    let prepared = execution
        .prepare_effective_geometry_replacement(object.node_id(), handle, &source, None)
        .unwrap();
    let lease = execution
        .commit_effective_content_replacement(prepared)
        .unwrap();
    drop(source);
    let before = execution.publication_context();
    {
        let live = scene.live(&mut execution);
        for layout in [
            live.effective_layout(&object).unwrap(),
            live.effective_family_layout(&family).unwrap(),
        ] {
            assert_eq!(layout.center, (0.0, 0.0));
            assert_eq!((layout.width, layout.height), (0.0, 0.0));
        }
        for anchor in [LayoutAnchor::from(&object), LayoutAnchor::from(&family)] {
            for dimension in [
                LayoutDimension::Width,
                LayoutDimension::Height,
                LayoutDimension::Depth,
            ] {
                assert_eq!(live.dimension_length(&anchor, dimension).unwrap(), 0.0);
            }
        }
    }
    assert_eq!(execution.publication_context(), before);
    execution.release_effective_content(lease).unwrap();
    assert_eq!(
        scene
            .live(&mut execution)
            .effective_layout(&object)
            .unwrap()
            .center,
        (4.0, 5.0)
    );
}

#[test]
fn authored_line_queries_reject_a_held_content_lease_and_recover_after_release() {
    let mut scene = Scene::new();
    let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
    scene.add(&line).unwrap();
    let mut execution = scene.execution_session().unwrap();
    let prepared = execution
        .prepare_effective_content_replacement(
            line.node_id(),
            ObjectContentRef::Geometry(GeometryRef::rectangle(4.0, 2.0)),
            None,
            None,
        )
        .unwrap();
    let lease = execution
        .commit_effective_content_replacement(prepared)
        .unwrap();
    {
        let live = scene.live(&mut execution);
        let layout = live.effective_layout(&line).unwrap();
        assert_eq!((layout.width, layout.height), (4.0, 2.0));
        assert!(matches!(
            live.effective_line_endpoints(&line),
            Err(LiveSessionError::Authoring(
                crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::EffectiveLineRenderOverride
                )
            ))
        ));
    }
    execution.release_effective_content(lease).unwrap();
    assert_eq!(
        scene
            .live(&mut execution)
            .effective_line_endpoints(&line)
            .unwrap(),
        ManimLineEndpoints {
            start: (-1.0, 0.0),
            end: (1.0, 0.0)
        }
    );
}
