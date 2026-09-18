//! Immutable text dependencies in the same resource scope as semantic geometry.
use super::SemanticStore;
use crate::{
    FontResourceArena, GeometryResource, GeometryResourceArena, TextResource, TextResourceArena,
    TextResourceHandle,
};

/// A compiled text payload rejected before any dependency is imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticTextImportError {
    Validation(crate::TextResourceValidationError),
    MissingFont(crate::FontResourceKey),
    Font(crate::FontResourceError),
    MissingGeometry(crate::GeometryResourceHandle),
    NonFiniteGeometry(crate::GeometryResourceHandle),
}

impl std::fmt::Display for SemanticTextImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(error) => error.fmt(f),
            Self::MissingFont(key) => write!(f, "missing text font dependency {key:?}"),
            Self::Font(error) => error.fmt(f),
            Self::MissingGeometry(handle) => write!(f, "missing text vector dependency {handle:?}"),
            Self::NonFiniteGeometry(handle) => {
                write!(f, "text vector dependency {handle:?} is not finite")
            }
        }
    }
}

impl std::error::Error for SemanticTextImportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Validation(error) => Some(error),
            Self::Font(error) => Some(error),
            _ => None,
        }
    }
}

impl SemanticStore {
    pub(super) fn forget_compiled_text_resource(
        &mut self,
        identity: &crate::TextCompilationIdentity,
    ) {
        if self.compiled_text_resources.remove(identity).is_some() {
            self.compiled_text_resource_retained_bytes = self
                .compiled_text_resource_retained_bytes
                .saturating_sub(compiled_identity_bytes(identity));
            if let Some(index) = self
                .compiled_text_resource_order
                .iter()
                .position(|candidate| candidate == identity)
            {
                self.compiled_text_resource_order.remove(index);
            }
        }
    }
    pub(super) fn remember_compiled_text_resource(
        &mut self,
        identity: crate::TextCompilationIdentity,
        handle: TextResourceHandle,
    ) {
        const MAX_COMPILED_TEXT_IDENTITIES: usize = 128;
        const MAX_COMPILED_TEXT_IDENTITY_BYTES: usize = 32 * 1024 * 1024;
        if self
            .compiled_text_resources
            .insert(identity.clone(), handle)
            .is_none()
        {
            self.compiled_text_resource_retained_bytes = self
                .compiled_text_resource_retained_bytes
                .saturating_add(compiled_identity_bytes(&identity));
            self.compiled_text_resource_order.push_back(identity);
        }
        while self.compiled_text_resource_order.len() > MAX_COMPILED_TEXT_IDENTITIES
            || self.compiled_text_resource_retained_bytes > MAX_COMPILED_TEXT_IDENTITY_BYTES
        {
            if let Some(expired) = self.compiled_text_resource_order.pop_front() {
                self.forget_compiled_text_resource(&expired);
            }
        }
    }
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
    ) -> Result<TextResourceHandle, SemanticTextImportError> {
        resource
            .validate()
            .map_err(SemanticTextImportError::Validation)?;
        for run in resource.runs.iter() {
            let incoming = fonts.get_for_face(&run.font).ok_or_else(|| {
                SemanticTextImportError::MissingFont(crate::FontResourceKey::from_face(&run.font))
            })?;
            if let Some(existing) = self.font_resources.get_for_face(&run.font) {
                if !std::sync::Arc::ptr_eq(&existing.data, &incoming.data)
                    && existing.data != incoming.data
                {
                    return Err(SemanticTextImportError::Font(
                        crate::FontResourceError::ConflictingResource(
                            crate::FontResourceKey::from_face(&run.font),
                        ),
                    ));
                }
            }
        }
        for vector in resource.vector_items.iter() {
            let GeometryResource::VectorPath(path) = geometries
                .get(vector.geometry)
                .ok_or(SemanticTextImportError::MissingGeometry(vector.geometry))?;
            if !path.is_finite() {
                return Err(SemanticTextImportError::NonFiniteGeometry(vector.geometry));
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

    /// Import a normalized compiled resource, or reuse its existing immutable
    /// payload in this semantic store. The key is compiler-owned complete input
    /// identity, never a presentation or node identity.
    pub fn import_compiled_text_resource(
        &mut self,
        identity: crate::TextCompilationIdentity,
        resource: TextResource,
        fonts: &FontResourceArena,
        geometries: &GeometryResourceArena,
    ) -> Result<TextResourceHandle, SemanticTextImportError> {
        if let Some(handle) = self.compiled_text_resources.get(&identity).copied() {
            if self.text_resources.get(handle).is_some() {
                return Ok(handle);
            }
            self.forget_compiled_text_resource(&identity);
        }
        let handle = self.import_text_resource(resource, fonts, geometries)?;
        self.remember_compiled_text_resource(identity, handle);
        Ok(handle)
    }
}

fn compiled_identity_bytes(identity: &crate::TextCompilationIdentity) -> usize {
    identity.descriptor.len().saturating_add(
        identity
            .font_contents
            .iter()
            .map(|font| font.len())
            .sum::<usize>(),
    )
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

    fn resource_counts(store: &SemanticStore) -> (usize, usize, usize) {
        (
            store.geometry_resources().len(),
            store.text_resources().len(),
            store.font_resources().len(),
        )
    }

    #[test]
    fn invalid_text_retains_validation_cause_before_resource_writes_and_recovers() {
        use std::error::Error;
        let mut store = SemanticStore::new();
        let mut invalid = empty_text();
        invalid.render_items = Arc::from([crate::TextRenderItem::Vector(0)]);
        let error = store
            .import_text_resource(
                invalid,
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap_err();
        assert!(matches!(error, SemanticTextImportError::Validation(_)));
        assert!(error
            .source()
            .unwrap()
            .is::<crate::TextResourceValidationError>());
        assert_eq!(resource_counts(&store), (0, 0, 0));
        store
            .import_text_resource(
                empty_text(),
                &FontResourceArena::new(),
                &GeometryResourceArena::new(),
            )
            .unwrap();
        assert_eq!(resource_counts(&store), (0, 1, 0));
    }

    #[test]
    fn missing_and_nonfinite_text_vectors_preserve_identity_and_import_nothing() {
        let mut source = GeometryResourceArena::new();
        let good = source.insert_path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 1.0)),
        );
        let bad =
            source.insert_path(crate::VectorPath::new().move_to(Vec2::new(f32::INFINITY, 0.0)));
        let vector = |geometry| crate::TextVectorItem {
            geometry,
            transform: crate::TextAffineTransform::IDENTITY,
            style: crate::TextVectorStyle::default(),
            source_span: None,
            semantic_key: None,
        };
        let mut resource = empty_text();
        resource.vector_items = Arc::from([vector(good), vector(bad)]);
        resource.render_items = Arc::from([
            crate::TextRenderItem::Vector(0),
            crate::TextRenderItem::Vector(1),
        ]);
        let mut store = SemanticStore::new();
        assert_eq!(
            store.import_text_resource(
                resource.clone(),
                &FontResourceArena::new(),
                &GeometryResourceArena::new()
            ),
            Err(SemanticTextImportError::MissingGeometry(good))
        );
        assert_eq!(resource_counts(&store), (0, 0, 0));
        assert_eq!(
            store.import_text_resource(resource, &FontResourceArena::new(), &source),
            Err(SemanticTextImportError::NonFiniteGeometry(bad))
        );
        // In particular, the first valid vector was not imported before the second failed.
        assert_eq!(resource_counts(&store), (0, 0, 0));
        let mut valid = empty_text();
        valid.vector_items = Arc::from([vector(good)]);
        valid.render_items = Arc::from([crate::TextRenderItem::Vector(0)]);
        store
            .import_text_resource(valid, &FontResourceArena::new(), &source)
            .unwrap();
        assert_eq!(resource_counts(&store), (1, 1, 0));
    }
}
