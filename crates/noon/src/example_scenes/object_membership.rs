//! Low-level semantic membership foundation on native and direct Rust/WASM.
//! High-level Mobject copy/affine/target semantics and Python exposure follow separately.
use crate::{ExecutionSession, Scene};
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
    scene.add(&tail)?;
    let mut session = scene.execution_session()?;
    let mut admit = SemanticMutationTransaction::new();
    admit.add_member(scene.root(), parent.node_id());
    session.apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), admit)?;
    let mut reorder = SemanticMutationTransaction::new();
    reorder.reorder_member(scene.root(), parent.node_id(), Some(tail.node_id()));
    session.apply_semantic_transaction_at_root(&mut store.borrow_mut(), scene.root(), reorder)?;
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
