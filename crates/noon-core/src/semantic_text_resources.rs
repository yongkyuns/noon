//! Immutable text dependencies in the same resource scope as semantic geometry.
use super::SemanticStore;
use crate::{
    FontResourceArena, GeometryResource, GeometryResourceArena, TextResource, TextResourceArena,
    TextResourceHandle,
};

impl SemanticStore {
    pub fn text_resources(&self) -> &TextResourceArena {
        &self.text_resources
    }
    pub fn font_resources(&self) -> &FontResourceArena {
        &self.font_resources
    }

    /// Import a compiled immutable payload once, before attaching its semantic
    /// object. Dependency validation precedes every resource write. This is
    /// resource registration, not a second authored scene or execution boundary.
    pub fn import_text_resource(
        &mut self,
        mut resource: TextResource,
        fonts: &FontResourceArena,
        geometries: &GeometryResourceArena,
    ) -> Result<TextResourceHandle, String> {
        resource.validate().map_err(|error| error.to_string())?;
        for run in resource.runs.iter() {
            let incoming = fonts
                .get_for_face(&run.font)
                .ok_or("missing text font dependency")?;
            if let Some(existing) = self.font_resources.get_for_face(&run.font) {
                if !std::sync::Arc::ptr_eq(&existing.data, &incoming.data)
                    && existing.data != incoming.data
                {
                    return Err(
                        "text font identity conflicts with registered immutable bytes".into(),
                    );
                }
            }
        }
        for vector in resource.vector_items.iter() {
            let GeometryResource::VectorPath(path) = geometries
                .get(vector.geometry)
                .ok_or("missing text vector dependency")?;
            if !path.is_finite() {
                return Err("text vector dependency is not finite".into());
            }
        }
        for run in resource.runs.iter() {
            let incoming = fonts
                .get_for_face(&run.font)
                .expect("font dependency preflighted");
            self.font_resources
                .intern_face(&run.font, incoming.data.clone())
                .expect("immutable font identity preflighted");
        }
        let mut imported = std::collections::HashMap::new();
        for vector in std::sync::Arc::make_mut(&mut resource.vector_items) {
            vector.geometry = *imported.entry(vector.geometry).or_insert_with(|| {
                let GeometryResource::VectorPath(path) = geometries
                    .get(vector.geometry)
                    .expect("vector dependency preflighted");
                self.geometry_resources.insert_path(path.as_ref().clone())
            });
        }
        Ok(self
            .text_resources
            .insert(resource)
            .expect("text resource preflighted"))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{Rect, TextSourceKind, Vec2};

    fn empty_text() -> TextResource {
        TextResource {
            source: Arc::from(""),
            kind: TextSourceKind::Plain,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    #[test]
    fn canonical_import_rebinds_text_to_the_target_store_arena() {
        let mut source_texts = TextResourceArena::new();
        let source = source_texts.insert(empty_text()).unwrap();
        let source_resource = source_texts.get(source).unwrap().clone();
        let mut target = SemanticStore::new();
        let local = target
            .import_text_resource(
                source_resource,
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();

        assert_ne!(source.arena, local.arena);
        assert!(target.text_resources().get(source).is_none());
        assert!(target.text_resources().get(local).is_some());
    }
}
