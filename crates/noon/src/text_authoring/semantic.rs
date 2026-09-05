//! Canonical text authoring in the same semantic store as geometry.
use super::{Text, TextAuthoringError, NATIVE_POINT_TO_SCENE_SCALE};
use noon_core::{GeometryResourceArena, Vec2};

impl crate::Scene {
    /// Create an ordinary detached text Mobject in this scene's shared store.
    pub fn text(&self, text: impl Into<Text>) -> Result<crate::Mobject, TextAuthoringError> {
        crate::Mobject::from_text(std::rc::Rc::clone(self.store()), text)
    }
}

impl crate::Mobject {
    /// Shape native text once and return an ordinary detached semantic Mobject.
    /// Add it to this scene with the same `add` operation used for geometry.
    pub fn from_text(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        text: impl Into<Text>,
    ) -> Result<crate::Mobject, TextAuthoringError> {
        let text = text.into();
        let mut transform = text.presentation.transform;
        transform.scale = transform.scale.component_mul(Vec2::new(
            NATIVE_POINT_TO_SCENE_SCALE,
            NATIVE_POINT_TO_SCENE_SCALE,
        ));
        let semantic_transform = noon_core::SemanticTransform2_5D {
            translation: noon_core::SemanticVec3::new(
                transform.translation.x as f64,
                transform.translation.y as f64,
                0.0,
            ),
            scale: noon_core::SemanticVec3::new(
                transform.scale.x as f64,
                transform.scale.y as f64,
                1.0,
            ),
            rotation_z: transform.rotation as f64,
        };
        if !semantic_transform.translation.is_finite()
            || !semantic_transform.scale.is_finite()
            || !semantic_transform.rotation_z.is_finite()
        {
            return Err(TextAuthoringError::Semantic(
                "text transform is not finite".into(),
            ));
        }
        let style = noon_core::SemanticStyle {
            fill: Some(noon_core::SemanticPaint::Solid(text.presentation.color)),
            fill_opacity: 1.0,
            stroke: None,
            stroke_width: 0.0,
            object_opacity: text.presentation.opacity as f64,
            ..Default::default()
        };
        if !style.is_finite() {
            return Err(TextAuthoringError::Semantic(
                "text style is not finite".into(),
            ));
        }
        // Ordinary plain text inherits live object style. Explicit styled spans
        // may override it, but an initial object color must not freeze glyph fill.
        let artifact = text.compile_artifact_with_fill(None)?;
        let handle = store
            .borrow_mut()
            .import_text_resource(
                artifact.resource,
                &artifact.fonts,
                &GeometryResourceArena::new(),
            )
            .map_err(TextAuthoringError::Semantic)?;
        let mut state = noon_core::SemanticObjectState::new(handle);
        state.transform = semantic_transform;
        state.style = style;
        crate::Mobject::new(store, state).map_err(TextAuthoringError::Semantic)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_text_uses_shared_semantic_identity_bounds_and_live_publication() {
        let mut scene = crate::Scene::new();
        let circle = scene.circle(0.5).unwrap();
        let mut label = scene.text(super::Text::new("Noon")).unwrap();
        assert!(label.width().unwrap() > 0.0);
        assert!(label.height().unwrap() > 0.0);
        assert_ne!(circle.node_id(), label.node_id());
        label.shift(1.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        scene.add(&label).unwrap();
        let mut session = scene.execution_session().unwrap();
        assert_eq!(session.frame().objects.len(), 2);
        assert!(session.frame().objects[0].geometry().is_some());
        assert!(session.frame().objects[1].text().is_some());
        let resource = label.state().unwrap().content.text().unwrap();
        let before = scene.store().borrow().text_resources().stats();
        assert!(scene
            .store()
            .borrow()
            .text_resources()
            .get(resource)
            .unwrap()
            .runs
            .iter()
            .all(|run| run.fill.is_none()));
        let mut live = scene.live(&mut session);
        live.set_translation(&label, 3.0, -1.0).unwrap();
        assert_eq!(
            live.effective(&label).unwrap().transform.translation,
            noon_core::Vec2::new(3.0, -1.0)
        );
        assert_eq!(
            live.authored(&label).unwrap().content.text(),
            Some(resource)
        );
        let mut style = live.authored(&label).unwrap().style;
        style.fill = Some(noon_core::SemanticPaint::Solid(noon_core::RED));
        live.replace_style(&label, style).unwrap();
        assert_eq!(
            live.effective(&label).unwrap().style.fill,
            Some(noon_core::RED)
        );
        assert_eq!(scene.store().borrow().text_resources().stats(), before);
    }

    #[test]
    fn invalid_text_presentation_registers_neither_resources_nor_semantic_nodes() {
        let scene = crate::Scene::new();
        let before = scene.store().borrow().scene_revision();
        let resources = scene.store().borrow().text_resources().stats();
        assert!(scene
            .text(super::Text::new("Noon").scale(f32::NAN))
            .is_err());
        assert!(scene
            .text(super::Text::new("Noon").color(noon_core::Color::rgba(f32::NAN, 1.0, 1.0, 1.0)))
            .is_err());
        assert_eq!(scene.store().borrow().scene_revision(), before);
        assert_eq!(scene.store().borrow().text_resources().stats(), resources);
    }
}
