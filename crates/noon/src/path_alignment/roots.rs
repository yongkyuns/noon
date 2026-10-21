//! Own-root path alignment shares preparation and publication with leaf edits.
use crate::{
    path_editing::{
        publish_running_path_edits, require_running_path_resource_admission, world_path,
        PreparedPathEdits,
    },
    AuthoringError, CompositeEntryHandle, ExecutionSession, LayoutAnchor, Mobject, MobjectTarget,
};
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore, VectorPath};
use std::{cell::RefCell, rc::Rc};

fn entry(anchor: &LayoutAnchor) -> Result<CompositeEntryHandle, AuthoringError> {
    CompositeEntryHandle::from_node(Rc::clone(anchor.integration_store()), anchor.resolve()?)
}

fn own_object(target: MobjectTarget<'_>) -> Result<Option<Mobject>, AuthoringError> {
    match target {
        MobjectTarget::Object(object) => Ok(Some(object.clone())),
        MobjectTarget::Family(family) => family.own_object(),
    }
}

fn capture(
    object: Option<&Mobject>,
    execution: Option<&ExecutionSession>,
) -> Result<Option<SemanticObjectState>, AuthoringError> {
    object
        .map(|object| match execution {
            Some(execution) => crate::effective_capture::capture_mobject_state(
                object.integration_store(),
                execution,
                object,
            ),
            None => object.state(),
        })
        .transpose()
}

fn point_count(
    object: Option<&Mobject>,
    execution: Option<&ExecutionSession>,
) -> Result<usize, AuthoringError> {
    match (object, execution) {
        (None, _) => Ok(0),
        (Some(object), Some(execution))
            if execution.semantic_object_is_reachable(object.node_id()) =>
        {
            crate::path_queries::effective_mobject_point_count(
                object.integration_store(),
                execution,
                object,
            )
        }
        (Some(object), _) => object.get_num_points(),
    }
}

fn center(
    anchor: &LayoutAnchor,
    execution: Option<&ExecutionSession>,
) -> Result<(f64, f64), AuthoringError> {
    match execution {
        Some(execution) => {
            let (_, bounds) = crate::family_layout::effective_anchor_layout_measure(
                anchor.integration_store(),
                execution,
                anchor,
                true,
            )?;
            Ok(crate::family_layout::bounds_critical_point(
                bounds, 0.0, 0.0,
            ))
        }
        None => anchor.layout().map(|layout| layout.center()),
    }
}

fn prepare(
    left: &LayoutAnchor,
    right: &LayoutAnchor,
    execution: Option<&ExecutionSession>,
    before_capture: impl FnOnce() -> Result<(), AuthoringError>,
) -> Result<PreparedPathEdits, AuthoringError> {
    if !Rc::ptr_eq(left.integration_store(), right.integration_store()) {
        return Err(AuthoringError::ForeignStore);
    }
    let entries = [entry(left)?, entry(right)?];
    if left.resolve()? == right.resolve()? {
        return PreparedPathEdits::prepare(&left.integration_store().borrow(), []);
    }
    let objects = [
        own_object(entries[0].as_target())?,
        own_object(entries[1].as_target())?,
    ];
    let counts = [
        point_count(objects[0].as_ref(), execution)?,
        point_count(objects[1].as_ref(), execution)?,
    ];
    // The own-count shortcut precedes capture and path conversion, including
    // point-free text/family roots and otherwise unchanged effective drivers.
    if counts[0] == counts[1] {
        return PreparedPathEdits::prepare(&left.integration_store().borrow(), []);
    }
    before_capture()?;
    let states = [
        capture(objects[0].as_ref(), execution)?,
        capture(objects[1].as_ref(), execution)?,
    ];
    let originals = {
        let store = left.integration_store().borrow();
        let path = |state: &Option<SemanticObjectState>| {
            state
                .as_ref()
                .map_or_else(|| Ok(VectorPath::new()), |state| world_path(&store, state))
        };
        [path(&states[0])?, path(&states[1])?]
    };
    let mut inputs = originals.clone();
    for (index, anchor) in [left, right].into_iter().enumerate() {
        if counts[index] == 0 {
            inputs[index] = VectorPath::from_manim_points_f64(vec![center(anchor, execution)?]);
        }
    }
    let (a, b) =
        noon_geometry::align_paths(&inputs[0], &inputs[1]).map_err(AuthoringError::PathQuery)?;
    let mut replacements = Vec::new();
    let mut initial = Vec::new();
    for (((entry, state), original), path) in
        entries.into_iter().zip(states).zip(originals).zip([a, b])
    {
        if path == original {
            continue;
        }
        let node = entry.as_target().node_id();
        match state {
            Some(state) => replacements.push((node, state, path)),
            None => initial.push((node, path)),
        }
    }
    let store = left.integration_store().borrow();
    PreparedPathEdits::prepare(&store, replacements)?.with_initial_family_paths(&store, initial)
}

impl LayoutAnchor {
    /// Align only the selected roots' own paths, retaining children and identity.
    pub fn align_path_points(&self, other: &Self) -> Result<(), AuthoringError> {
        let prepared = prepare(self, other, None, || Ok(()))?;
        prepared.publish(
            &mut self.integration_store().borrow_mut(),
            |store, transaction| transaction.apply(store).map_err(AuthoringError::from),
        )?;
        Ok(())
    }
}

/// Align root paths through one existing coherent execution/resource publication.
pub fn publish_root_alignment(
    store: &Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    execution: &mut ExecutionSession,
    left: &LayoutAnchor,
    right: &LayoutAnchor,
) -> Result<(), AuthoringError> {
    if !Rc::ptr_eq(store, left.integration_store()) || !Rc::ptr_eq(store, right.integration_store())
    {
        return Err(AuthoringError::ForeignStore);
    }
    execution
        .require_published_store(&store.borrow())
        .map_err(AuthoringError::from)?;
    let prepared = prepare(left, right, Some(execution), || {
        require_running_path_resource_admission(store, root, execution)
    })?;
    if !prepared.has_changes() {
        return Ok(());
    }
    publish_running_path_edits(store, root, execution, prepared)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Scene};

    #[test]
    fn empty_family_alignment_keeps_payload_members_resources_and_revision_untouched() {
        let mut scene = Scene::new();
        let child = scene.circle(0.25).unwrap();
        let left = scene.family(&[(&child).into()]).unwrap();
        let right = scene.family(&[]).unwrap();
        let revision = scene.revision();
        let nodes = scene.integration_store().borrow().len();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        LayoutAnchor::from(&left)
            .align_path_points(&LayoutAnchor::from(&right))
            .unwrap();
        assert!(left.own_object().unwrap().is_none());
        assert!(right.own_object().unwrap().is_none());
        assert_eq!(left.member_count().unwrap(), 1);
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
    }

    #[test]
    fn first_root_alignment_uses_child_center_without_changing_children_or_presentation() {
        let mut scene = Scene::new();
        let mut child = scene.circle(0.25).unwrap();
        child.shift(-2.0, 1.0).unwrap();
        let root = scene
            .family_with_presentation(&[(&child).into()], 3.0, Color::RED, false)
            .unwrap();
        let other = scene.circle(0.5).unwrap();
        let child_before = child.state().unwrap();
        let style = root.own_style().unwrap();
        let nodes = scene.integration_store().borrow().len();
        let revision = scene.revision();
        LayoutAnchor::from(&root)
            .align_path_points(&LayoutAnchor::from(&other))
            .unwrap();
        let points = root.own_path_query().unwrap().points().unwrap();
        assert_eq!(points.len(), other.get_num_points().unwrap());
        assert!(points.iter().all(|p| *p == (-2.0, 1.0)));
        assert_eq!(root.own_style().unwrap(), style);
        assert!(!root.draw_enabled().unwrap());
        assert_eq!(root.member_count().unwrap(), 1);
        assert_eq!(child.state().unwrap(), child_before);
        assert_eq!(scene.integration_store().borrow().len(), nodes);
        assert_eq!(scene.revision(), revision.checked_next().unwrap());
    }

    #[test]
    fn live_mixed_root_alignment_publishes_once_and_rejects_stale_noop_reads() {
        let mut scene = Scene::new();
        let mut child = scene.square(0.25).unwrap();
        child.shift(3.0, 2.0).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        let other = scene.circle(0.5).unwrap();
        scene
            .add_many(&[(&family).into(), (&other).into()])
            .unwrap();
        let mut execution = scene.execution_session().unwrap();
        let revision = scene.revision();
        scene
            .live(&mut execution)
            .align_path_points(&LayoutAnchor::from(&other), &LayoutAnchor::from(&family))
            .unwrap();
        assert_eq!(scene.revision(), revision.checked_next().unwrap());
        assert!(family
            .effective_own_path_query(&execution)
            .unwrap()
            .points()
            .unwrap()
            .iter()
            .all(|p| *p == (3.0, 2.0)));
        let empty = scene.family(&[]).unwrap(); // Deliberately stale execution.
        let context = execution.publication_context();
        assert!(scene
            .live(&mut execution)
            .align_path_points(&LayoutAnchor::from(&empty), &LayoutAnchor::from(&empty),)
            .is_err());
        assert_eq!(execution.publication_context(), context);
        assert!(empty.own_object().unwrap().is_none());
    }

    #[test]
    fn owned_family_pairs_and_aliases_preserve_identity_and_foreign_pairs_reject() {
        let mut scene = Scene::new();
        let family = scene.family(&[]).unwrap();
        family
            .set_points_f64(&[(0., 0.), (1., 0.), (1., 1.), (2., 1.)])
            .unwrap();
        let other = scene.circle(0.5).unwrap();
        let peer = scene.family(&[]).unwrap();
        peer.set_points_f64(&other.path_query().unwrap().points().unwrap())
            .unwrap();
        let id = family.node_id();
        LayoutAnchor::from(&family)
            .align_path_points(&LayoutAnchor::from(&peer))
            .unwrap();
        assert_eq!(
            family.get_num_points().unwrap(),
            peer.get_num_points().unwrap()
        );
        assert_eq!(family.node_id(), id);
        let revision = scene.revision();
        LayoutAnchor::from(&family)
            .align_path_points(&LayoutAnchor::from(&family))
            .unwrap();
        assert_eq!(scene.revision(), revision);
        let foreign = Scene::new().family(&[]).unwrap();
        assert!(LayoutAnchor::from(&family)
            .align_path_points(&LayoutAnchor::from(&foreign))
            .is_err());
        assert_eq!(scene.revision(), revision);
    }
}
