//! Own point content stays on the addressed family node and uses normal path edits.
mod append;
mod empty_arrays;
mod xyz;
use crate::{AuthoringError, Mobject, MobjectFamily, PathQuery};
use noon_core::{
    SemanticMutationTransaction, SemanticStore, SemanticTransform2_5D, StoredGeometry, Vec2,
    VectorPath,
};
use std::rc::Rc;

impl crate::Scene {
    /// Clear a family's own points while keeping its identity and members.
    pub fn clear_family_points(&mut self, family: &MobjectFamily) -> Result<(), AuthoringError> {
        crate::MobjectTarget::from(family).require_store(self.integration_store())?;
        let object = family.own_object()?;
        self.require_resource_admission()?;
        match object {
            Some(object) => self.clear_points(&object),
            None => Ok(()),
        }
    }

    fn set_initial_family_path(
        &mut self,
        family: &MobjectFamily,
        path: VectorPath,
    ) -> Result<(), AuthoringError> {
        self.with_semantic_publication(|store, publish| {
            publish_initial_family_path(store, family.node_id(), path, publish).map(|_| ())
        })
    }

    /// Read the root's own effective path, excluding all descendants.
    pub fn effective_family_own_path_query(
        &self,
        family: &MobjectFamily,
    ) -> Result<PathQuery, AuthoringError> {
        crate::MobjectTarget::from(family).require_store(self.integration_store())?;
        let execution = self.running_execution().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        family.effective_own_path_query(execution)
    }
}

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
    pub(crate) fn effective_own_path_query(
        &self,
        execution: &crate::ExecutionSession,
    ) -> Result<PathQuery, AuthoringError> {
        let own = self.own_object()?;
        execution
            .require_published_store(&self.integration_store().borrow())
            .map_err(AuthoringError::from)?;
        match own {
            Some(object) if execution.semantic_object_is_reachable(self.node_id()) => {
                crate::path_queries::effective_path_query(
                    self.integration_store(),
                    execution,
                    &object,
                )
            }
            // Detached roots have no execution row. Their authored payload is
            // still observable after the same publication/coherence validation.
            _ => self.own_path_query(),
        }
    }

    /// Replace this root's owned cubic point array in authored state. The
    /// existing root style, presentation, identity and members are retained.
    /// Use Scene publication for changes after execution bootstrap.
    pub fn set_points_f64(&self, points: &[(f64, f64)]) -> Result<(), AuthoringError> {
        self.set_points_3d(&crate::path_editing::promote_raw_points_3d(points))
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
    use super::*;
    use crate::{MobjectTarget, Scene};

    #[test]
    fn family_presentation_constructor_is_atomic_before_and_after_bootstrap() {
        for running in [false, true] {
            for draw_enabled in [false, true] {
                let mut scene = Scene::new();
                let child = scene.square(0.5).unwrap();
                let child_before = child.state().unwrap();
                if running {
                    let execution = scene.execution_session().unwrap();
                    scene.install_execution(execution);
                }
                let revision = scene.revision();
                let family = scene
                    .family_with_presentation(
                        &[(&child).into()],
                        3.0,
                        crate::Color::GREEN,
                        draw_enabled,
                    )
                    .unwrap();
                assert_eq!(scene.revision(), revision.checked_next().unwrap());
                assert_eq!(family.draw_enabled().unwrap(), draw_enabled);
                assert_eq!(family.own_color().unwrap(), crate::Color::GREEN);
                assert_eq!(crate::LayoutAnchor::from(&family).z_index().unwrap(), 3.0);
                assert_eq!(family.member_count().unwrap(), 1);
                assert_eq!(child.state().unwrap(), child_before);
                let revision = scene.revision();
                let nodes = scene.integration_store().borrow().len();
                assert!(scene
                    .family_with_presentation(
                        &[(&child).into()],
                        f64::NAN,
                        crate::Color::GREEN,
                        draw_enabled,
                    )
                    .is_err());
                assert_eq!(scene.revision(), revision);
                assert_eq!(scene.integration_store().borrow().len(), nodes);
                scene
                    .set_family_points_f64(&family, &[(0., 0.), (1., 1.)])
                    .unwrap();
                assert_eq!(family.draw_enabled().unwrap(), draw_enabled);
            }
        }
    }

    #[test]
    fn root_points_reuse_scene_path_edits_and_roll_back_resources() {
        for attached in [false, true] {
            for materialized in [false, true] {
                let mut scene = Scene::new();
                let child = scene.square(0.5).unwrap();
                let family = scene
                    .family_with_presentation(&[(&child).into()], 2.0, crate::Color::GREEN, false)
                    .unwrap();
                if materialized {
                    family.set_points_f64(&[(0., 0.), (1., 1.)]).unwrap();
                    family.shift(3., 2.).unwrap();
                }
                let child_before = child.state().unwrap();
                if attached {
                    scene.add_many(&[(&family).into()]).unwrap();
                }
                let execution = scene.execution_session().unwrap();
                scene.install_execution(execution);
                let store = std::rc::Rc::clone(scene.integration_store());
                let nodes = store.borrow().len();
                let points = [(0., 0.), (1., 1.), (2., 1.), (3., 0.), (4., 2.)];
                let revision = scene.revision();
                if !materialized {
                    scene.clear_family_points(&family).unwrap();
                    assert!(family.own_object().unwrap().is_none());
                    assert_eq!(scene.revision(), revision);
                }
                scene.set_family_points_f64(&family, &points).unwrap();
                assert_eq!(
                    scene
                        .effective_family_own_path_query(&family)
                        .unwrap()
                        .points()
                        .unwrap(),
                    points
                );
                assert_eq!(family.get_num_points().unwrap(), 5);
                assert!(!family.draw_enabled().unwrap());
                assert_eq!(family.member_count().unwrap(), 1);
                assert_eq!(store.borrow().len(), nodes);
                assert_eq!(child.state().unwrap(), child_before);
                let revision = scene.revision();
                let resources = store.borrow().geometry_resources().len();
                let publication = scene.owned_execution().publication_context();
                assert!(scene
                    .set_family_points_f64(&family, &[(f64::NAN, 0.)])
                    .is_err());
                let foreign = Scene::new().family(&[]).unwrap();
                assert!(scene.set_family_points_f64(&foreign, &points).is_err());
                assert_eq!(scene.revision(), revision);
                assert_eq!(store.borrow().geometry_resources().len(), resources);
                assert_eq!(scene.owned_execution().publication_context(), publication);
                scene.clear_family_points(&family).unwrap();
                assert_eq!(
                    scene
                        .effective_family_own_path_query(&family)
                        .unwrap()
                        .point_count(),
                    0
                );
                assert_eq!(store.borrow().len(), nodes);
                assert_eq!(child.state().unwrap(), child_before);
                // A raw arena edit makes the running Scene's publication stale.
                let empty =
                    MobjectFamily::create(Rc::clone(scene.integration_store()), &[]).unwrap();
                let resources = store.borrow().geometry_resources().len();
                assert!(scene.set_family_points_f64(&empty, &points).is_err());
                assert!(scene.clear_family_points(&empty).is_err());
                assert!(empty.own_object().unwrap().is_none());
                assert_eq!(store.borrow().geometry_resources().len(), resources);
            }
        }
    }

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
        assert_eq!(
            tree.points
                .iter()
                .map(|point| point.xy())
                .collect::<Vec<_>>(),
            points
        );
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
    use crate::{AnimationOptions, Color, RateFunction, Scene};

    #[test]
    fn own_root_query_validates_empty_publications_and_observes_live_content_only() {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        let detached = scene.family(&[]).unwrap();
        let points = [(0., 0.), (1., 1.), (2., 1.), (3., 0.)];
        detached.set_points_f64(&points).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        assert_eq!(scene.own_point_count((&family).into()).unwrap(), 0);
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
        assert_eq!(scene.own_point_count((&family).into()).unwrap(), 0);
        assert_eq!(scene.own_point_count((&detached).into()).unwrap(), 4);
        scene.set_family_points_f64(&family, &points).unwrap();
        assert_eq!(scene.own_point_count((&family).into()).unwrap(), 4);
        assert_eq!(
            family
                .effective_own_path_query(scene.owned_execution())
                .unwrap()
                .points()
                .unwrap(),
            points
        );
        family.clear_points().unwrap();
        assert!(scene.own_point_count((&family).into()).is_err());
        assert!(scene.own_point_count((&detached).into()).is_err());
        assert!(family
            .effective_own_path_query(scene.owned_execution())
            .is_err());
    }

    #[test]
    fn own_point_count_follows_family_root_reveal() {
        let mut scene = Scene::new();
        let family = scene
            .family_with_presentation(&[], 0.0, Color::WHITE, true)
            .unwrap();
        family
            .set_points_f64(&[
                (0., 0.),
                (1., 0.),
                (2., 0.),
                (3., 0.),
                (3., 0.),
                (4., 0.),
                (5., 0.),
                (6., 0.),
            ])
            .unwrap();
        let object = family.own_object().unwrap().unwrap();
        let mut execution = scene.into_execution().unwrap();
        let segment = execution
            .live()
            .declare_and_activate_create(
                &object,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        execution.live().advance_segment_to(segment, 0.25).unwrap();
        assert_eq!(family.get_num_points().unwrap(), 8);
        assert_eq!(
            execution.scene().own_point_count((&family).into()).unwrap(),
            4
        );
    }
}
