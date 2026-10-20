//! Own point content stays on the addressed family node and uses normal path edits.
use crate::{AuthoringError, Mobject, MobjectFamily, PathQuery};
use noon_core::{
    SemanticMutationTransaction, SemanticStore, SemanticTransform2_5D, StoredGeometry, Vec2,
    VectorPath,
};
use std::rc::Rc;

impl MobjectFamily {
    /// A typed view of this root's own payload, if materialized. Membership,
    /// identity and ownership remain on the same semantic node.
    pub fn own_object(&self) -> Result<Option<Mobject>, AuthoringError> {
        let has_content = self
            .integration_store()
            .borrow()
            .semantic_family_checked(self.node_id())?
            .semantic_object_state()
            .is_some();
        if has_content {
            Mobject::from_node(Rc::clone(self.integration_store()), self.node_id()).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Own point cardinality, excluding all descendants.
    pub fn get_num_points(&self) -> Result<usize, AuthoringError> {
        self.own_object()?
            .map_or(Ok(0), |object| object.get_num_points())
    }

    /// Snapshot the root's own world-space path, excluding descendants.
    /// An unmaterialized root has an empty derived query and allocates no node.
    pub fn own_path_query(&self) -> Result<PathQuery, AuthoringError> {
        match self.own_object()? {
            Some(object) => object.path_query(),
            None => PathQuery::prepare(&VectorPath::new(), SemanticTransform2_5D::default()),
        }
    }

    /// Read only the root's own current path through a coherent execution.
    /// Empty roots validate the same store/publication contract as owned roots.
    pub fn effective_own_path_query(
        &self,
        execution: &crate::ExecutionSession,
    ) -> Result<PathQuery, AuthoringError> {
        match self.own_object()? {
            Some(object) => crate::path_queries::effective_path_query(
                self.integration_store(),
                execution,
                &object,
            ),
            None => {
                execution
                    .require_published_store(&self.integration_store().borrow())
                    .map_err(AuthoringError::from)?;
                self.own_path_query()
            }
        }
    }

    /// Replace this root's owned cubic point array in authored state. The
    /// existing root style, presentation, identity and members are retained.
    /// Use Scene/LiveSession publication for changes after execution bootstrap.
    pub fn set_points_f64(&self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
        if let Some(mut object) = self.own_object()? {
            return object.set_points_f64(points);
        }
        let path = crate::path_editing::raw_points_path(points)?;
        publish_initial_family_path(
            &mut self.integration_store().borrow_mut(),
            self.node_id(),
            path,
            |store, transaction| {
                transaction
                    .apply(store)
                    .map(|_| ())
                    .map_err(AuthoringError::from)
            },
        )
    }

    pub fn set_points(&self, points: &[Vec2]) -> Result<(), AuthoringError> {
        self.set_points_f64(
            &points
                .iter()
                .map(|point| (f64::from(point.x), f64::from(point.y)))
                .collect::<Vec<_>>(),
        )
    }

    /// Clear only this root's owned points; child content and membership stay intact.
    pub fn clear_points(&self) -> Result<(), AuthoringError> {
        match self.own_object()? {
            Some(mut object) => object.clear_points(),
            None => Ok(()),
        }
    }
}

pub(crate) fn publish_initial_family_path<T>(
    store: &mut SemanticStore,
    root: noon_core::SemanticNodeId,
    path: VectorPath,
    publish: impl FnOnce(&mut SemanticStore, SemanticMutationTransaction) -> Result<T, AuthoringError>,
) -> Result<T, AuthoringError> {
    store.semantic_family_checked(root)?;
    store.with_geometry_path(path, |store, resource| {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(root, StoredGeometry::Resource(resource));
        publish(store, transaction)
    })
}

#[cfg(test)]
mod tests {
    use crate::{MobjectTarget, Scene};

    #[test]
    fn root_points_share_identity_preserve_paint_and_leave_children_untouched() {
        let mut scene = Scene::new();
        let child = scene.square(0.4).unwrap();
        let group = scene.family(&[(&child).into()]).unwrap();
        group.set_own_color(1., 0., 0., 1.).unwrap();
        let style = group.own_style().unwrap();
        let child_before = child.state().unwrap();
        let count = scene.integration_store().borrow().len();
        let points = [
            (0., 0.),
            (0.1 + f64::EPSILON, 0.),
            (0.2, 0.3),
            (0.3, 0.4),
            (0.5, 0.6),
        ];
        group.set_points_f64(&points).unwrap();
        assert_eq!(group.get_num_points().unwrap(), 5);
        assert_eq!(group.own_path_query().unwrap().points().unwrap(), points);
        assert_eq!(
            group.own_object().unwrap().unwrap().node_id(),
            group.node_id()
        );
        assert_eq!(group.own_style().unwrap(), style);
        assert_eq!(group.member_count().unwrap(), 1);
        assert_eq!(scene.integration_store().borrow().len(), count);
        assert_eq!(child.state().unwrap(), child_before);
        let tree = MobjectTarget::from(&group).point_reduction_tree().unwrap();
        assert_eq!(tree.points, points);
        assert_eq!(tree.children.len(), 1);
        group.clear_points().unwrap();
        assert_eq!(group.get_num_points().unwrap(), 0);
        assert_eq!(group.member_count().unwrap(), 1);
        assert_eq!(child.state().unwrap(), child_before);
    }

    #[test]
    fn copy_retains_owned_root_content_and_independent_children() {
        let mut scene = Scene::new();
        let child = scene.square(0.4).unwrap();
        let group = scene.family(&[(&child).into()]).unwrap();
        let points = [(0., 0.), (1., 0.), (1., 1.), (2., 1.), (3., 2.)];
        group.set_points_f64(&points).unwrap();
        group.set_own_color(0.2, 0.4, 0.6, 0.8).unwrap();
        group.shift(2., 3.).unwrap();
        let count = scene.integration_store().borrow().len();
        let original = group.own_object().unwrap().unwrap().state().unwrap();
        let copied = group.copy_family().unwrap();
        let root = copied.root();
        assert_ne!(root.node_id(), group.node_id());
        assert_eq!(root.get_num_points().unwrap(), 5);
        assert_eq!(
            root.own_path_query().unwrap().points().unwrap(),
            group.own_path_query().unwrap().points().unwrap()
        );
        let state = root.own_object().unwrap().unwrap().state().unwrap();
        assert_eq!(state.content, original.content);
        assert_eq!(state.transform, original.transform);
        assert_eq!(state.style, original.style);
        assert_eq!(state.role(), original.role());
        assert_eq!(state.signal_bindings(), original.signal_bindings());
        assert_eq!(state.sheen_direction, original.sheen_direction);
        assert_eq!(scene.integration_store().borrow().len(), count + 2);
        assert_ne!(
            root.member_at(0).unwrap().unwrap().as_target().node_id(),
            child.node_id()
        );
        group.clear_points().unwrap();
        assert_eq!(root.get_num_points().unwrap(), 5);
        assert_eq!(group.get_num_points().unwrap(), 0);
    }

    #[test]
    fn unmaterialized_queries_and_clear_are_read_only_and_invalid_points_are_atomic() {
        let mut scene = Scene::new();
        let group = scene.family(&[]).unwrap();
        let count = scene.integration_store().borrow().len();
        let revision = scene.integration_store().borrow().scene_revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        assert_eq!(group.get_num_points().unwrap(), 0);
        assert_eq!(group.own_path_query().unwrap().point_count(), 0);
        group.clear_points().unwrap();
        assert!(group.set_points_f64(&[(f64::NAN, 0.)]).is_err());
        assert!(group.own_object().unwrap().is_none());
        assert_eq!(scene.integration_store().borrow().len(), count);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            revision
        );
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
    fn replacing_world_points_reuses_the_shared_edit_and_keeps_noop_resources() {
        let mut scene = Scene::new();
        let group = scene.family(&[]).unwrap();
        let points = [(0., 0.), (1., 0.), (1., 1.), (2., 1.)];
        group.set_points_f64(&points).unwrap();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        group.set_points_f64(&points).unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
        group.shift(2., 3.).unwrap();
        group.set_points_f64(&points).unwrap();
        assert_eq!(group.own_path_query().unwrap().points().unwrap(), points);
        assert_eq!(
            group
                .own_object()
                .unwrap()
                .unwrap()
                .state()
                .unwrap()
                .transform,
            noon_core::SemanticTransform::default()
        );
    }
}

#[cfg(all(test, feature = "diagnostics"))]
mod reference_tests;

#[cfg(test)]
mod effective_query_tests {
    use crate::Scene;

    #[test]
    fn own_root_query_validates_empty_publications_and_observes_live_content_only() {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let execution = scene.execution_session().unwrap();
        assert!(family
            .effective_own_path_query(&execution)
            .unwrap()
            .points()
            .unwrap()
            .is_empty());
        let foreign = Scene::new().execution_session().unwrap();
        assert!(family.effective_own_path_query(&foreign).is_err());
        scene.install_execution(execution);
        let points = [(0., 0.), (1., 1.), (2., 1.), (3., 0.)];
        scene.set_family_points_f64(&family, &points).unwrap();
        assert_eq!(
            family
                .effective_own_path_query(scene.owned_execution())
                .unwrap()
                .points()
                .unwrap(),
            points
        );
        family.clear_points().unwrap();
        assert!(family
            .effective_own_path_query(scene.owned_execution())
            .is_err());
    }
}
