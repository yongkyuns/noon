//! Cold and live glyph-text admission coupled to one semantic publication.
use super::{SemanticStore, SemanticTextImportError};
use crate::{
    FontFaceIdentity, FontResourceKey, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult,
};
use crate::{FontResourceArena, TextResource, TextResourceHandle};
use std::{collections::BTreeMap, sync::Arc};

type PreparedGlyphFonts = BTreeMap<FontResourceKey, (FontFaceIdentity, Arc<[u8]>)>;

impl SemanticStore {
    /// Publish exactly one detached glyph-text object through a caller-owned
    /// cold or live semantic publication route.
    ///
    /// Every text and font dependency is validated before a resource is inserted.
    /// The fresh text handle is usable only to build this helper's one detached
    /// object; it cannot be attached or applied to an existing object. On rejection
    /// the handle is removed generationally, while fonts remain staged. Fonts enter
    /// the append-only arena only after success.
    ///
    /// Vector text requires the geometry-aware import path instead.
    pub fn publish_glyph_detached_text<T, E>(
        &mut self,
        resource: TextResource,
        fonts: FontResourceArena,
        build_state: impl FnOnce(TextResourceHandle) -> crate::SemanticObjectState,
        publish: impl FnOnce(&mut Self, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        let mut inputs = Vec::new();
        inputs.try_reserve_exact(1)?;
        inputs.push((resource, fonts));
        self.with_glyph_text_resources(inputs, |store, handles| {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(crate::SemanticNodeCreation::object(build_state(handles[0])));
            publish(store, transaction)
        })
    }

    fn with_glyph_text_resources<T, E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        publish: impl FnOnce(&mut Self, &[TextResourceHandle]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        let fonts = self.preflight_glyph_text_resources(&inputs)?;
        let mut handles = Vec::new();
        handles.try_reserve_exact(inputs.len())?;
        for (resource, _) in inputs {
            handles.push(
                self.text_resources
                    .insert(resource)
                    .expect("text batch preflighted"),
            );
        }

        let result = publish(self, &handles);
        if result.is_err() {
            for handle in handles {
                self.text_resources
                    .remove(handle.id)
                    .expect("fresh unpublished text is removable");
            }
        } else {
            for (_, (face, data)) in fonts {
                self.font_resources
                    .intern_face(&face, data)
                    .expect("font batch preflighted");
            }
        }
        result
    }

    /// Publish glyph-only text resources and one cold semantic transaction.
    ///
    /// This is deliberately NOT a live-execution publication callback. Live
    /// detached construction uses [`Self::publish_glyph_detached_text`].
    pub fn apply_glyph_text_transaction<E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        build: impl FnOnce(&[TextResourceHandle]) -> Result<SemanticMutationTransaction, E>,
    ) -> Result<SemanticMutationTransactionResult, E>
    where
        E: From<SemanticTextImportError>
            + From<SemanticMutationTransactionError>
            + From<std::collections::TryReserveError>,
    {
        self.with_glyph_text_resources(inputs, |store, handles| {
            build(handles).and_then(|transaction| transaction.apply(store).map_err(E::from))
        })
    }

    fn preflight_glyph_text_resources(
        &self,
        inputs: &[(TextResource, FontResourceArena)],
    ) -> Result<PreparedGlyphFonts, SemanticTextImportError> {
        let mut fonts = BTreeMap::new();
        for (resource, source) in inputs {
            resource
                .validate()
                .map_err(SemanticTextImportError::Validation)?;
            if let Some(vector) = resource.vector_items.first() {
                return Err(SemanticTextImportError::MissingGeometry(vector.geometry));
            }
            for run in resource.runs.iter() {
                let key = FontResourceKey::from_face(&run.font);
                let incoming = source
                    .get_for_face(&run.font)
                    .ok_or_else(|| SemanticTextImportError::MissingFont(key.clone()))?;
                let existing = self
                    .font_resources
                    .get_for_face(&run.font)
                    .map(|font| &font.data)
                    .or_else(|| fonts.get(&key).map(|(_, data)| data));
                if let Some(existing) = existing {
                    if !Arc::ptr_eq(existing, &incoming.data) && *existing != incoming.data {
                        return Err(SemanticTextImportError::Font(
                            crate::FontResourceError::ConflictingResource(key),
                        ));
                    }
                }
                fonts
                    .entry(key)
                    .or_insert_with(|| (run.font.clone(), incoming.data.clone()));
            }
        }
        Ok(fonts)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::Arc};

    use super::*;
    use crate::{
        Color, FontFaceIdentity, GlyphRun, PositionedGlyph, Rect, SemanticObjectState,
        SemanticStyle, TextAffineTransform, TextClusterIdentity, TextDirection, TextPart,
        TextRenderItem, TextSourceKind, TextSourceSpan, Vec2,
    };

    #[derive(Debug)]
    enum Error {
        Import(SemanticTextImportError),
        Transaction(SemanticMutationTransactionError),
        Allocation(std::collections::TryReserveError),
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Import(error) => error.fmt(formatter),
                Self::Transaction(error) => error.fmt(formatter),
                Self::Allocation(error) => error.fmt(formatter),
            }
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::Import(error) => Some(error),
                Self::Transaction(error) => Some(error),
                Self::Allocation(error) => Some(error),
            }
        }
    }

    impl From<SemanticTextImportError> for Error {
        fn from(value: SemanticTextImportError) -> Self {
            Self::Import(value)
        }
    }

    impl From<SemanticMutationTransactionError> for Error {
        fn from(value: SemanticMutationTransactionError) -> Self {
            Self::Transaction(value)
        }
    }

    impl From<std::collections::TryReserveError> for Error {
        fn from(value: std::collections::TryReserveError) -> Self {
            Self::Allocation(value)
        }
    }

    fn glyph_resource() -> (TextResource, FontResourceArena) {
        let font = FontFaceIdentity {
            family: Arc::from("test"),
            face_key: Arc::from("test-face"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let mut fonts = FontResourceArena::new();
        fonts
            .intern_face(&font, Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();
        let span = TextSourceSpan::new(0, 1);
        let glyph = PositionedGlyph {
            glyph_id: 1,
            cluster: TextClusterIdentity {
                source_span: span,
                cluster_ordinal: 0,
                semantic_key: None,
            },
            origin: Vec2::ZERO,
            advance: Vec2::new(1.0, 0.0),
            bounds: Rect::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
        };
        (
            TextResource {
                source: Arc::from("x"),
                kind: TextSourceKind::Plain,
                runs: Arc::from([GlyphRun {
                    font,
                    variations: Arc::from([]),
                    font_size: 1.0,
                    direction: TextDirection::LeftToRight,
                    fill: Some(Color::WHITE),
                    stroke: None,
                    transform: TextAffineTransform::IDENTITY,
                    glyphs: Arc::from([glyph]),
                }]),
                vector_items: Arc::from([]),
                render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
                parts: Arc::from([TextPart {
                    source_span: span,
                    first_cluster: 0,
                    cluster_count: 1,
                    first_vector: 0,
                    vector_count: 0,
                    semantic_key: None,
                }]),
                bounds: Rect::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
                baseline: 0.0,
                layout_artifact: None,
            },
            fonts,
        )
    }

    #[test]
    fn rejected_detached_publication_releases_provisional_text_and_stages_fonts() {
        let (resource, fonts) = glyph_resource();
        let mut store = SemanticStore::new();
        let before_text = store.text_resources().stats();
        let before_fonts = store.font_resources().stats();
        let provisional = Cell::new(None);

        let error = store
            .publish_glyph_detached_text(
                resource,
                fonts,
                |handle| {
                    provisional.set(Some(handle));
                    let mut state = SemanticObjectState::new(handle);
                    state.style = SemanticStyle {
                        object_opacity: f64::NAN,
                        ..Default::default()
                    };
                    state
                },
                |store, transaction| {
                    let handle = provisional
                        .get()
                        .expect("state builder observed an inserted text resource");
                    assert!(store.text_resources().get(handle).is_some());
                    transaction.apply(store).map_err(Error::Transaction)
                },
            )
            .unwrap_err();
        assert!(matches!(error, Error::Transaction(_)));
        let handle = provisional
            .get()
            .expect("publisher received fresh text handle");
        assert!(store.text_resources().get(handle).is_none());
        assert_eq!(store.text_resources().stats(), before_text);
        assert_eq!(store.font_resources().stats(), before_fonts);
    }
}
