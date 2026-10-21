//! Public child edits and alias-preserving copies, paired with
//! `web/python/examples/manim_parity_object_members.py` on the shared renderer.
use crate::{ExecutionSession, ManimGeometryOptions, Mobject, Scene};

fn split_ids(root: &Mobject) -> Result<Vec<noon_core::SemanticNodeId>, crate::AuthoringError> {
    root.split_members().map(|members| {
        members
            .iter()
            .map(|member| member.as_target().node_id())
            .collect()
    })
}

/// Build the same authored/returned scene for native and direct Rust/WASM hosts.
pub fn session() -> Result<ExecutionSession, String> {
    build().map_err(|error| error.to_string())
}

fn build() -> Result<ExecutionSession, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let root = scene.geometry(ManimGeometryOptions::circle(0.2)?)?;
    let mut a = scene.geometry(ManimGeometryOptions::square(0.1)?)?;
    let mut b = scene.geometry(ManimGeometryOptions::circle(0.1)?)?;
    let mut c = scene.geometry(ManimGeometryOptions::square(0.15)?)?;
    a.shift(-1.0, 0.0)?;
    b.shift(1.0, 0.0)?;
    c.shift(0.0, 1.0)?;
    let original_root = root.node_id();
    root.add_many(&[(&a).into(), (&b).into(), (&a).into()])?;
    assert_eq!(split_ids(&root)?, [original_root, b.node_id(), a.node_id()]);
    let captured = root.split_iter()?;
    root.add((&c).into())?;
    let captured_ids = captured
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .map(|member| member.as_target().node_id())
        .collect::<Vec<_>>();
    assert_eq!(captured_ids, [original_root, b.node_id(), a.node_id()]);
    root.remove((&a).into())?;
    let captured = root.split_iter()?;
    root.prepend_many(&[(&c).into(), (&b).into(), (&c).into()])?;
    assert_eq!(captured.collect::<Result<Vec<_>, _>>()?.len(), 1);
    assert_eq!(split_ids(&root)?, [original_root, c.node_id(), b.node_id()]);
    // The Python metadata alias is an ordinary retained native identity here.
    let metadata_alias = b.clone();
    scene.add(&root)?;
    let mut session = scene.execution_session()?;
    {
        let mut live = scene.live(&mut session);
        let first = live.wait_segment(0.25)?;
        live.advance_segment_to(first, first.end_time())?;
        live.complete_segment(first)?;
        live.replace_members((&root).into(), &[(&a).into(), (&b).into()])?;
        let copied = live.copy_mobject_with_references(&root, &[(&metadata_alias).into()])?;
        let copied_root = copied.root();
        let copied_alias = copied.mobject(&metadata_alias)?;
        assert_eq!(
            copied_root.member_at(1)?.unwrap().as_target().node_id(),
            copied_alias.node_id()
        );
        assert_ne!(
            copied_root.member_at(0)?.unwrap().as_target().node_id(),
            a.node_id()
        );
        live.shift(copied_root, 0.0, -1.0)?;
        assert_eq!(split_ids(&root)?, [original_root, a.node_id(), b.node_id()]);
        live.add(copied_root)?;
        let second = live.wait_segment(0.25)?;
        live.advance_segment_to(second, second.end_time())?;
        live.complete_segment(second)?;
    }
    assert_eq!(root.node_id(), original_root);
    assert_eq!(session.frame().time, 0.5);
    assert_eq!(session.painter_order().len(), 6);
    Ok(session)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_public_object_membership_scene_completes_both_segments() {
        let session = super::session().unwrap();
        assert_eq!(session.frame().time, 0.5);
        assert_eq!(session.painter_order().len(), 6);
    }
}
