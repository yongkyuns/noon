use super::*;
use crate::arrow_authoring::publish_arrow_options;
use crate::{ManimArrow, ManimArrowOptions};

impl LiveSession<'_> {
    /// Create an Arrow family through the current running publication. The
    /// returned family remains detached until it is explicitly added.
    pub fn create_manim_arrow(
        &mut self,
        options: ManimArrowOptions,
    ) -> Result<ManimArrow, LiveSessionError> {
        let committed = self.with_semantic_publication(|store, publish| {
            publish_arrow_options(options, store, publish)
        })?;
        ManimArrow::from_committed(Rc::clone(self.store), committed).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AnimationOptions, MobjectTarget, Scene};

    #[test]
    fn arrow_created_after_wait_is_detached_and_usable_by_live_family_transform() {
        let mut scene = Scene::new();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.5).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();

        let arrow = live
            .create_manim_arrow(ManimArrowOptions::arrow(0.0, 0.0, 1.0, 0.0).unwrap())
            .unwrap();
        let store = scene.integration_store().borrow();
        assert_eq!(
            store.node(scene.root()).unwrap().members(),
            Vec::<noon_core::SemanticNodeId>::new()
        );
        assert!(store.node(arrow.family().node_id()).is_some());
        drop(store);

        live.add_many(&[MobjectTarget::Family(arrow.family())])
            .unwrap();
        let target = live
            .create_manim_arrow(ManimArrowOptions::arrow(0.0, 0.0, 2.0, 0.0).unwrap())
            .unwrap();
        let segment = live
            .declare_and_activate_family_transform_to(
                arrow.family(),
                target.family(),
                AnimationOptions::new().run_time(0.25),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert!(live.effective(arrow.shaft()).is_ok());
    }

    #[test]
    fn invalid_live_arrow_request_does_not_publish_any_state() {
        let mut scene = Scene::new();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let revision = scene.revision();
        let member_count = scene
            .integration_store()
            .borrow()
            .node(scene.root())
            .unwrap()
            .member_count();

        let node_count = scene.integration_store().borrow().len();
        let mut options = ManimArrowOptions::arrow(0.0, 0.0, 1.0, 0.0).unwrap();
        options.set_tip_length(f64::MAX).unwrap();
        options
            .set_max_tip_length_to_length_ratio(f64::MAX)
            .unwrap();
        assert!(live.create_manim_arrow(options).is_err());
        assert_eq!(scene.revision(), revision);
        assert_eq!(scene.integration_store().borrow().len(), node_count);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .node(scene.root())
                .unwrap()
                .member_count(),
            member_count
        );
        // The same live publication remains usable after the rejected request.
        assert!(live
            .create_manim_arrow(ManimArrowOptions::arrow(0.0, 0.0, 1.0, 0.0).unwrap())
            .is_ok());
    }
}
