//! Low-level semantic membership foundation on native and direct Rust/WASM.
//! Authored and coherent live copies retain own content and ordered descendants.
//! Broader affine/state semantics and Python membership exposure follow separately.
use crate::{ExecutionSession, MobjectTarget, Scene};
use noon_core::SemanticMutationTransaction;

pub fn session() -> Result<ExecutionSession, String> {
    build().map_err(|error| error.to_string())
}

fn build() -> Result<ExecutionSession, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut parent = scene.square(2.0)?;
    let mut child = scene.circle(0.6)?;
    let mut tail = scene.square(1.0)?;
    parent.set_fill(0.2, 0.4, 1.0, 1.0)?;
    child.set_fill(1.0, 0.8, 0.1, 1.0)?;
    tail.set_fill(0.2, 0.8, 0.4, 1.0)?;
    parent.set_stroke_width(0.0)?;
    child.set_stroke_width(0.0)?;
    tail.set_stroke_width(0.0)?;
    parent.shift(-1.5, 0.0)?;
    child.shift(-0.8, 0.3)?;
    tail.shift(1.5, 0.0)?;
    let store = scene.integration_store().clone();
    let mut membership = SemanticMutationTransaction::new();
    membership.add_member(parent.node_id(), child.node_id());
    membership.apply(&mut store.borrow_mut())?;
    let child_pose = child.state()?.transform.translation;
    parent.shift(1.0, -0.25)?;
    assert_eq!(child.state()?.transform.translation.x, child_pose.x + 1.0);
    assert_eq!(child.state()?.transform.translation.y, child_pose.y - 0.25);
    parent.shift(-1.0, 0.25)?;
    assert_eq!(child.state()?.transform.translation, child_pose);
    let authored = parent.copy_with_references(&[])?;
    let child = authored.mobject(&child)?;
    let parent = authored.root().clone();
    scene.add(&tail)?;
    let mut session = scene.execution_session()?;
    let mut admit = SemanticMutationTransaction::new();
    admit.add_member(scene.root(), parent.node_id());
    session.apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), admit)?;
    let mut reorder = SemanticMutationTransaction::new();
    reorder.reorder_member(scene.root(), parent.node_id(), Some(tail.node_id()));
    session.apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), reorder)?;
    let copied = scene.live(&mut session).copy_mobject(&parent)?;
    let copied_child = copied.mobject(&child)?;
    let copied_parent = copied.root().clone();
    assert_eq!(session.frame().objects.len(), 3);
    let mut replace = SemanticMutationTransaction::new();
    replace.remove_member(scene.root(), parent.node_id());
    replace.add_member(scene.root(), copied_parent.node_id());
    replace.reorder_member(scene.root(), copied_parent.node_id(), Some(tail.node_id()));
    session.apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), replace)?;
    let parent = copied_parent;
    let child = copied_child;
    let baseline = scene.live(&mut session).effective_layout(&parent)?;
    let child_pose = child.state()?.transform.translation;
    let shifted = scene.live(&mut session).shift(&parent, 1.0, -0.25)?;
    assert_eq!(shifted.impacts().len(), 2);
    assert_eq!(child.state()?.transform.translation.x, child_pose.x + 1.0);
    let shifted = scene.live(&mut session).effective_layout(&parent)?;
    assert!((shifted.center.0 - baseline.center.0 - 1.0).abs() < 1e-6);
    assert!((shifted.center.1 - baseline.center.1 + 0.25).abs() < 1e-6);
    scene.live(&mut session).shift(&parent, -1.0, 0.25)?;
    assert_eq!(child.state()?.transform.translation, child_pose);
    let target = MobjectTarget::from(&parent);
    let own_points = parent.path_query()?.points()?;
    let child_points = child.path_query()?.points()?;
    let expected_points = own_points
        .iter()
        .chain(&child_points)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(target.all_points()?, expected_points);
    let tree = target.point_reduction_tree()?;
    assert_eq!(tree.points, own_points);
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].points, child_points);
    assert_eq!(tree.flattened_points(), expected_points);
    let expected_anchors = parent
        .path_query()?
        .anchors()?
        .into_iter()
        .chain(child.path_query()?.anchors()?)
        .collect::<Vec<_>>();
    assert_eq!(target.vector_boundary_points()?, expected_anchors);
    assert_eq!(
        session
            .painter_order()
            .iter()
            .map(|&row| session.frame().objects[row as usize].id)
            .collect::<Vec<_>>(),
        [parent.node_id(), child.node_id(), tail.node_id()]
            .map(noon_compile::semantic_execution_object_id)
    );
    let mut live = scene.live(&mut session);
    let wait = live.wait_segment(0.2)?;
    live.advance_segment_to(wait, wait.end_time())?;
    live.complete_segment(wait)?;
    Ok(session)
}
