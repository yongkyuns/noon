//! Root drawing eligibility is shared presentation state, independent from paint.
use crate::{AuthoringError, LayoutAnchor, Mobject, MobjectFamily, Scene};
use noon_core::SemanticMutationTransaction;
use std::rc::Rc;

impl LayoutAnchor {
    /// Read this root's eligibility; descendants retain their own eligibility.
    pub fn draw_enabled(&self) -> Result<bool, AuthoringError> {
        let id = self.resolve()?;
        self.integration_store()
            .borrow()
            .node(id)
            .and_then(|node| node.presentation())
            .map(|presentation| presentation.draw_enabled)
            .ok_or_else(|| {
                noon_core::SemanticSceneOperationError::NotSemanticAuthoringNode(id).into()
            })
    }

    /// Change this root's authored drawing capability without changing points or paint.
    /// Running scenes should use `Scene::set_draw_enabled` for coherent publication.
    pub fn set_draw_enabled(&self, enabled: bool) -> Result<(), AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_draw_enabled(self.resolve()?, enabled);
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

impl Mobject {
    pub fn draw_enabled(&self) -> Result<bool, AuthoringError> {
        LayoutAnchor::from(self).draw_enabled()
    }
    pub fn set_draw_enabled(&self, enabled: bool) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).set_draw_enabled(enabled)
    }
}

impl MobjectFamily {
    pub fn draw_enabled(&self) -> Result<bool, AuthoringError> {
        LayoutAnchor::from(self).draw_enabled()
    }
    pub fn set_draw_enabled(&self, enabled: bool) -> Result<(), AuthoringError> {
        LayoutAnchor::from(self).set_draw_enabled(enabled)
    }
}

impl Scene {
    /// Publish only the addressed root's eligibility through the normal transaction.
    pub fn set_draw_enabled(
        &mut self,
        source: &LayoutAnchor,
        enabled: bool,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), source.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_draw_enabled(source.resolve()?, enabled);
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Set a family's own cubic points, including its first payload after bootstrap.
    /// Resources, semantic content and execution membership publish atomically.
    pub fn set_family_points_f64(
        &mut self,
        family: &MobjectFamily,
        points: &[(f64, f64)],
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), family.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        if self.running_execution().is_some() {
            return self.with_running_execution(|store, root, execution| {
                crate::family_points::publish_family_points(store, root, execution, family, points)
            });
        }
        family.set_points_f64(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_compile::semantic_execution_object_id;
    use noon_render_wgpu::{text::TextDeviceMetrics, RetainedFramePreparer};
    const POINTS: [(f64, f64); 4] = [(-2., 0.), (-1., 1.), (1., 1.), (2., 0.)];

    fn row(scene: &Scene, node: noon_core::SemanticNodeId) -> usize {
        scene
            .owned_execution()
            .frame()
            .objects
            .iter()
            .position(|row| row.id == semantic_execution_object_id(node))
            .unwrap()
    }

    fn render(
        scene: &mut Scene,
        renderer: &mut RetainedFramePreparer,
        device: &wgpu::Device,
    ) -> (usize, noon_runtime::FrameChanges) {
        let publication = scene.owned_execution_mut().take_renderer_publication();
        let prepared = renderer
            .prepare_publication(
                device,
                &publication,
                TextDeviceMetrics::uniform(32.0).unwrap(),
            )
            .unwrap();
        (
            prepared.geometry_stats().instance_count,
            publication.changes().clone(),
        )
    }

    #[test]
    fn family_root_drawing_is_independent_from_points_children_and_copies() {
        for materialized in [false, true] {
            let mut scene = Scene::new();
            let child = scene.square(0.5).unwrap();
            let family = scene.family(&[(&child).into()]).unwrap();
            family.set_draw_enabled(false).unwrap();
            let style = family.own_style().unwrap();
            if materialized {
                family.set_points_f64(&POINTS).unwrap();
            }
            family.shift(1.0, 0.0).unwrap();
            assert!(!family.draw_enabled().unwrap());
            assert_eq!(family.own_style().unwrap(), style);
            assert!(child.draw_enabled().unwrap());
            assert_eq!(
                family.get_num_points().unwrap(),
                if materialized { 4 } else { 0 }
            );
            let copy = family.copy_family().unwrap();
            assert!(!copy.root().draw_enabled().unwrap());
            assert!(copy.mobject(&child).unwrap().draw_enabled().unwrap());
            assert_eq!(
                copy.root().get_num_points().unwrap(),
                family.get_num_points().unwrap()
            );
        }
    }

    #[test]
    fn native_family_root_lowering_and_local_draw_toggle_preserve_query_geometry() {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        family.set_points_f64(&POINTS).unwrap();
        family.set_draw_enabled(false).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        let index = row(&scene, family.node_id());
        assert_eq!(scene.owned_execution().frame().objects.len(), 2);
        let before = scene.owned_execution().frame().objects[index].clone();
        let bounds = scene.effective_family_layout(&family).unwrap();
        let (device, _queue) = wgpu::Device::noop(&Default::default());
        let mut renderer = RetainedFramePreparer::new();
        assert_eq!(render(&mut scene, &mut renderer, &device).0, 1);
        scene.owned_execution_mut().take_frame_changes();
        scene
            .set_draw_enabled(&LayoutAnchor::from(&family), true)
            .unwrap();
        let (draw_count, changed) = render(&mut scene, &mut renderer, &device);
        assert_eq!(changed.object_indices(), &[index]);
        let frame = scene.owned_execution().frame();
        let mut wanted = before;
        wanted.draw_enabled = true;
        assert_eq!(frame.objects[index], wanted);
        assert_eq!(draw_count, 2);
        let current = scene.effective_family_layout(&family).unwrap();
        assert_eq!(
            (current.center, current.width, current.height),
            (bounds.center, bounds.width, bounds.height)
        );
        assert_eq!(family.own_path_query().unwrap().points().unwrap(), POINTS);
        assert_eq!(scene.owned_execution().last_patch_stats().full_seeks, 0);
        assert_eq!(
            scene
                .owned_execution()
                .last_patch_stats()
                .full_group_rebuilds,
            0
        );
        let context = scene.owned_execution().publication_context();
        scene
            .set_draw_enabled(&LayoutAnchor::from(&family), true)
            .unwrap();
        assert_eq!(scene.owned_execution().publication_context(), context);
        assert!(scene.owned_execution_mut().take_frame_changes().is_empty());
        let meshes = renderer.resident_path_mesh_count();
        scene
            .set_draw_enabled(&LayoutAnchor::from(&family), false)
            .unwrap();
        assert_eq!(render(&mut scene, &mut renderer, &device).0, 1);
        assert_eq!(renderer.resident_path_mesh_count(), meshes);
        let current = scene.effective_family_layout(&family).unwrap();
        assert_eq!(
            (current.center, current.width, current.height),
            (bounds.center, bounds.width, bounds.height)
        );
    }

    #[test]
    fn first_running_family_payload_enters_locally_then_exits_and_reenters_with_root() {
        let mut scene = Scene::new();
        let child = scene.square(0.5).unwrap();
        let family = scene.family(&[(&child).into()]).unwrap();
        family.set_draw_enabled(false).unwrap();
        scene.add_many(&[(&family).into()]).unwrap();
        let execution = scene.execution_session().unwrap();
        scene.install_execution(execution);
        assert_eq!(scene.owned_execution().frame().objects.len(), 1);
        scene.owned_execution_mut().take_frame_changes();
        scene.set_family_points_f64(&family, &POINTS).unwrap();
        assert_eq!(scene.owned_execution().frame().objects.len(), 2);
        let root_index = row(&scene, family.node_id());
        let child_index = row(&scene, child.node_id());
        assert!(!scene.owned_execution().frame().is_drawable(root_index));
        assert!(scene.owned_execution().frame().is_drawable(child_index));
        let changes = scene.owned_execution_mut().take_frame_changes();
        assert_eq!(changes.added_indices(), &[root_index]);
        assert_eq!(
            scene.owned_execution().painter_order(),
            &[root_index as u32, child_index as u32]
        );
        let mut paint = SemanticMutationTransaction::new();
        paint.set_family_color(family.node_id(), noon_core::Color::RED);
        scene.apply_semantic_transaction(paint).unwrap();
        assert_eq!(
            scene.owned_execution().frame().objects[root_index]
                .style
                .stroke,
            Some(noon_core::Color::RED)
        );
        let mut remove = SemanticMutationTransaction::new();
        remove.remove_member(scene.root(), family.node_id());
        scene.apply_semantic_transaction(remove).unwrap();
        assert!(!scene.owned_execution().frame().is_present(root_index));
        assert!(!scene.owned_execution().frame().is_present(child_index));
        let mut add = SemanticMutationTransaction::new();
        add.add_member(scene.root(), family.node_id());
        scene.apply_semantic_transaction(add).unwrap();
        assert!(scene.owned_execution().frame().is_present(root_index));
        assert!(scene.owned_execution().frame().is_present(child_index));
        assert_eq!(scene.owned_execution().frame().objects.len(), 2);
        assert_eq!(row(&scene, family.node_id()), root_index);
    }
}
