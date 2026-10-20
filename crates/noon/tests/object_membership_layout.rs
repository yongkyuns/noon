use noon::{
    AnimationOptions, LayoutAnchor, LiveLayoutTarget, Mobject, MobjectFamily, RateFunction, Scene,
};
use noon_core::{SemanticMutationTransaction, SemanticNodeId};

fn attach(scene: &Scene, parent: SemanticNodeId, children: &[SemanticNodeId]) {
    let mut tx = SemanticMutationTransaction::new();
    for &child in children {
        tx.add_member(parent, child);
    }
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
}

fn root_tree(scene: &mut Scene) -> (Mobject, Mobject, Mobject, MobjectFamily) {
    let root = scene.square(2.0).unwrap();
    let mut child = scene.square(2.0).unwrap();
    let mut grandchild = scene.square(2.0).unwrap();
    child.shift(4.0, 0.0).unwrap();
    grandchild.shift(8.0, 0.0).unwrap();
    let nested = scene
        .family(&[(&child).into(), (&grandchild).into()])
        .unwrap();
    attach(scene, child.node_id(), &[grandchild.node_id()]);
    attach(scene, root.node_id(), &[child.node_id(), nested.node_id()]);
    (root, child, grandchild, nested)
}

fn translation(object: &Mobject) -> (f64, f64) {
    let point = object.state().unwrap().transform.translation;
    (point.x, point.y)
}

#[test]
fn ordinary_root_dimensions_and_critical_points_include_own_content_and_descendants() {
    let mut scene = Scene::new();
    let (root, _, _, _) = root_tree(&mut scene);
    let revision = scene.revision();
    assert_eq!((root.width().unwrap(), root.height().unwrap()), (10.0, 2.0));
    assert_eq!(root.center().unwrap(), (4.0, 0.0));
    assert_eq!(root.critical_point(1.0, 0.0).unwrap(), (9.0, 0.0));
    let layout = LayoutAnchor::from(&root).layout().unwrap();
    assert_eq!(
        (layout.width(), layout.height(), layout.center()),
        (10.0, 2.0, (4.0, 0.0))
    );
    assert_eq!(scene.revision(), revision);
    let family = scene.family(&[(&root).into()]).unwrap();
    let after_creation = scene.revision();
    assert_eq!(
        (
            family.layout().unwrap().width(),
            family.layout().unwrap().center()
        ),
        (10.0, (4.0, 0.0))
    );
    assert_eq!(scene.revision(), after_creation);
}

#[test]
fn object_member_anchor_indexes_own_points_before_children_and_rejects_bad_indices_without_edits() {
    let mut scene = Scene::new();
    let (root, child, _, nested) = root_tree(&mut scene);
    let before = scene.revision();
    assert_eq!(
        LayoutAnchor::from(&root)
            .member(0)
            .resolve_checked()
            .unwrap(),
        root.node_id()
    );
    assert_eq!(
        LayoutAnchor::from(&root)
            .member(1)
            .resolve_checked()
            .unwrap(),
        child.node_id()
    );
    assert_eq!(
        LayoutAnchor::from(&root)
            .member(-1)
            .resolve_checked()
            .unwrap(),
        nested.node_id()
    );
    assert_eq!(
        LayoutAnchor::from(&root)
            .member(1)
            .layout()
            .unwrap()
            .width(),
        6.0
    );
    assert!(LayoutAnchor::from(&root)
        .member(3)
        .resolve_checked()
        .is_err());
    assert!(LayoutAnchor::from(&root)
        .member(-4)
        .resolve_checked()
        .is_err());
    assert_eq!(scene.revision(), before);
}

#[test]
fn authored_root_shift_and_move_translate_unique_content_once_and_leave_other_roots_alone() {
    let mut scene = Scene::new();
    let (mut root, child, grandchild, _) = root_tree(&mut scene);
    let mut unrelated = scene.square(1.0).unwrap();
    unrelated.shift(40.0, 3.0).unwrap();
    let revision = scene.revision();
    root.shift(2.0, -1.0).unwrap();
    assert_eq!(scene.revision(), revision.checked_next().unwrap());
    assert_eq!(
        [
            translation(&root),
            translation(&child),
            translation(&grandchild)
        ],
        [(2.0, -1.0), (6.0, -1.0), (10.0, -1.0)]
    );
    assert_eq!(translation(&unrelated), (40.0, 3.0));
    root.move_to(3.0, 2.0).unwrap();
    assert_eq!(
        [
            translation(&root),
            translation(&child),
            translation(&grandchild)
        ],
        [(-1.0, 2.0), (3.0, 2.0), (7.0, 2.0)]
    );
    assert_eq!(root.center().unwrap(), (3.0, 2.0));
}

#[test]
fn root_shift_rejects_late_descendant_overflow_without_partial_mutation() {
    let mut scene = Scene::new();
    let mut root = scene.square(1.0).unwrap();
    let mut child = scene.square(1.0).unwrap();
    child.set_translation(f64::from(f32::MAX), 0.0).unwrap();
    attach(&scene, root.node_id(), &[child.node_id()]);
    let before = scene.revision();
    let states = [root.state().unwrap(), child.state().unwrap()];
    assert!(root.shift(f64::from(f32::MAX), 0.0).is_err());
    assert_eq!(scene.revision(), before);
    assert_eq!([root.state().unwrap(), child.state().unwrap()], states);
}

#[test]
fn effective_root_layout_reads_each_current_descendant_pose_without_publication() {
    let mut scene = Scene::new();
    let (root, _, grandchild, _) = root_tree(&mut scene);
    let mut target = grandchild.target_editor().unwrap();
    target.shift(4.0, 0.0).unwrap();
    scene.add(&root).unwrap();
    let animation = scene
        .declare_transform_to(
            &grandchild,
            &target,
            AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();
    let mut execution = scene.execution_session().unwrap();
    let segment = scene
        .live(&mut execution)
        .play_animation(&animation)
        .unwrap();
    scene
        .live(&mut execution)
        .advance_segment_to(segment, 1.0)
        .unwrap();
    for time in [1.0, 0.25, 1.5, 0.0, 1.0] {
        execution.seek(time).unwrap();
        let before = execution.publication_context();
        let layout = scene.live(&mut execution).effective_layout(&root).unwrap();
        assert_eq!(layout.width, 10.0 + 2.0 * time);
        assert_eq!(layout.center, (4.0 + time, 0.0));
        if time == 1.0 {
            // A driven descendant cannot be baked into a persistent root move.
            assert!(scene
                .live(&mut execution)
                .move_to(
                    &root,
                    LiveLayoutTarget::Point(0.0, 0.0),
                    (0.0, 0.0),
                    (1.0, 1.0),
                )
                .is_err());
        }
        assert_eq!(execution.publication_context(), before);
        assert_eq!(root.center().unwrap(), (4.0, 0.0));
    }
}

#[test]
fn live_root_shift_and_move_publish_once_for_unique_descendants_and_use_whole_root_bounds() {
    let mut scene = Scene::new();
    let (root, child, grandchild, _) = root_tree(&mut scene);
    let unrelated = scene.square(1.0).unwrap();
    scene.add(&root).unwrap();
    scene.add(&unrelated).unwrap();
    let mut execution = scene.execution_session().unwrap();
    execution.take_frame_changes();
    let before = scene.revision();
    let result = scene.live(&mut execution).shift(&root, 2.0, -1.0).unwrap();
    assert_eq!(result.impacts().len(), 3);
    assert_eq!(scene.revision(), before.checked_next().unwrap());
    assert_eq!(
        [
            translation(&root),
            translation(&child),
            translation(&grandchild)
        ],
        [(2.0, -1.0), (6.0, -1.0), (10.0, -1.0)]
    );
    assert_eq!(execution.take_frame_changes().object_indices(), &[0, 1, 2]);
    let result = scene
        .live(&mut execution)
        .move_to(
            &root,
            LiveLayoutTarget::Point(3.0, 2.0),
            (0.0, 0.0),
            (1.0, 1.0),
        )
        .unwrap();
    assert_eq!(result.impacts().len(), 3);
    assert_eq!(
        [
            translation(&root),
            translation(&child),
            translation(&grandchild)
        ],
        [(-1.0, 2.0), (3.0, 2.0), (7.0, 2.0)]
    );
    assert_eq!(translation(&unrelated), (0.0, 0.0));
    assert_eq!(
        scene
            .live(&mut execution)
            .effective_layout(&root)
            .unwrap()
            .center,
        (3.0, 2.0)
    );
}

#[test]
fn empty_own_points_index_the_first_child_without_a_self_entry() {
    let mut scene = Scene::new();
    let root = scene
        .path(noon::VectorPath::new(), Default::default())
        .unwrap();
    let mut child = scene.square(2.0).unwrap();
    child.shift(4.0, 0.0).unwrap();
    attach(&scene, root.node_id(), &[child.node_id()]);
    assert_eq!(
        LayoutAnchor::from(&root)
            .member(0)
            .resolve_checked()
            .unwrap(),
        child.node_id()
    );
    assert_eq!(root.center().unwrap(), (4.0, 0.0));
    assert_eq!(root.width().unwrap(), 2.0);
    assert!(LayoutAnchor::from(&root)
        .member(1)
        .resolve_checked()
        .is_err());
}

#[test]
fn root_dimensions_include_cubic_handles_while_center_and_edges_use_anchors() {
    let mut scene = Scene::new();
    let curve = |x| {
        noon::VectorPath::new()
            .move_to(noon::Vec2::new(x, 1.0))
            .cubic_to(
                noon::Vec2::new(x + 9.0, 5.0),
                noon::Vec2::new(x + 7.0, -4.0),
                noon::Vec2::new(x + 1.0, 2.0),
            )
    };
    let root = scene.path(curve(-3.0), Default::default()).unwrap();
    let child = scene.path(curve(12.0), Default::default()).unwrap();
    attach(&scene, root.node_id(), &[child.node_id()]);
    assert_eq!((root.width().unwrap(), root.height().unwrap()), (24.0, 9.0));
    assert_eq!(root.center().unwrap(), (5.0, 1.5));
    assert_eq!(root.critical_point(1.0, 0.0).unwrap(), (13.0, 1.5));
    let family = scene.family(&[(&root).into()]).unwrap();
    assert_eq!(
        (
            family.layout().unwrap().width(),
            family.layout().unwrap().center()
        ),
        (24.0, (5.0, 1.5))
    );
}

#[test]
fn an_entirely_empty_root_scope_has_no_geometry_bounds_or_nonzero_center() {
    let mut scene = Scene::new();
    let root = scene
        .path(noon::VectorPath::new(), Default::default())
        .unwrap();
    let mut child = scene
        .path(noon::VectorPath::new(), Default::default())
        .unwrap();
    child.set_translation(4.0, 2.0).unwrap();
    attach(&scene, root.node_id(), &[child.node_id()]);
    assert_eq!(root.layout_bounds().unwrap(), None);
    assert_eq!(root.center().unwrap(), (0.0, 0.0));
    assert_eq!(root.width().unwrap(), 0.0);
    let mut execution = scene.execution_session().unwrap();
    let before = execution.publication_context();
    let effective = scene.live(&mut execution).effective_layout(&root).unwrap();
    assert_eq!(effective.center, (0.0, 0.0));
    assert_eq!(effective.width, 0.0);
    assert_eq!(execution.publication_context(), before);
}
