use super::*;
use crate::{StyleUpdate, SvgDefaultStyle, SvgImportOptions};

impl LiveSession<'_> {
    /// Parse and publish a detached SVG family through this running session.
    /// All immutable path resources and semantic identities share one rollback
    /// scope; no partial family survives a failed running publication.
    pub fn create_svg(
        &mut self,
        source: &str,
        options: SvgImportOptions,
        svg_default: Option<SvgDefaultStyle>,
    ) -> Result<MobjectFamily, LiveSessionError> {
        let wrapped = svg_default
            .map(|style| crate::svg_default_style::source_with_svg_default(source, style))
            .transpose()
            .map_err(LiveSessionError::Svg)?;
        let source = wrapped.as_deref().unwrap_or(source);
        let mut store = self.store.borrow_mut();
        self.session
            .require_resource_creation_at_root(&store, self.root)?;
        let family = crate::svg_authoring::publish_svg_creation(
            &mut store,
            source,
            options,
            StyleUpdate::default(),
            None,
            |store, transaction| {
                self.session
                    .apply_semantic_transaction_at_root(store, self.root, transaction)
                    .map_err(crate::AuthoringError::from)
            },
        )
        .map_err(LiveSessionError::Svg)?;
        drop(store);
        MobjectFamily::from_node(Rc::clone(self.store), family).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{integration::AuthoringFailure, Scene};

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red" fill-opacity=".3"/></svg>"#;

    #[test]
    fn svg_after_wait_is_detached_then_uses_existing_live_family_edits() {
        let scene = Scene::new();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
        let before = scene.revision();
        let svg = live
            .create_svg(SVG, SvgImportOptions::default(), None)
            .unwrap();
        assert_eq!(scene.revision(), before.checked_next().unwrap());
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .node(scene.root())
                .unwrap()
                .member_count(),
            0
        );
        let member = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(svg.node_id())
            .unwrap()[0];
        let object = Mobject::from_node(Rc::clone(scene.integration_store()), member).unwrap();
        assert_eq!(object.state().unwrap().style.fill_opacity, 76.0 / 255.0);
        live.add_many(&[MobjectTarget::Family(&svg)]).unwrap();
        live.shift_family(&svg, 1.0, 0.0).unwrap();
        live.set_family_fill(&svg, Some(Color::from_hex(0x00FF00)), Some(0.25))
            .unwrap();
        let layout = live.effective_family_layout(&svg).unwrap();
        assert!((layout.center.0 - 1.0).abs() < 1e-6);
        assert_eq!(
            live.effective_fill_color(&object).unwrap(),
            Some(Color::from_hex(0x00FF00))
        );
        assert_eq!(live.effective(&object).unwrap().fill_opacity(), 0.25);
    }

    #[test]
    fn rejected_live_svg_preserves_state_resources_and_typed_cause() {
        let scene = Scene::new();
        let mut session = scene.execution_session().unwrap();
        let before = (
            scene.revision(),
            scene.integration_store().borrow().len(),
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
        );
        let mut live = scene.live(&mut session);
        let error = live
            .create_svg("<svg", SvgImportOptions::default(), None)
            .unwrap_err();
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "live.svg");
        assert_eq!(failure.cause.unwrap().code, "svg.invalid_xml");
        assert_eq!(
            before,
            (
                scene.revision(),
                scene.integration_store().borrow().len(),
                scene
                    .integration_store()
                    .borrow()
                    .geometry_resources()
                    .len()
            )
        );
        assert!(live
            .create_svg(SVG, SvgImportOptions::default(), None)
            .is_ok());
    }
}
