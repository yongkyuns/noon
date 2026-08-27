use std::fmt;

use crate::{
    remove_text_if_current, replace_text_if_current, FontFaceIdentity, FontResourceArena,
    GeometryResourceArena, GeometryResourceHandle, TextResource, TextResourceArena,
    TextResourceError, TextResourceHandle, TextResourceMutationError,
};

#[derive(Clone, Debug, PartialEq)]
pub enum TextResourceTransactionMutation {
    Replace {
        expected: TextResourceHandle,
        resource: TextResource,
    },
    Remove {
        expected: TextResourceHandle,
    },
}

impl TextResourceTransactionMutation {
    pub const fn expected(&self) -> TextResourceHandle {
        match self {
            Self::Replace { expected, .. } | Self::Remove { expected } => *expected,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextResourceMutationTransaction {
    mutations: Vec<TextResourceTransactionMutation>,
}

impl TextResourceMutationTransaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace(&mut self, expected: TextResourceHandle, resource: TextResource) -> &mut Self {
        self.mutations
            .push(TextResourceTransactionMutation::Replace { expected, resource });
        self
    }

    pub fn remove(&mut self, expected: TextResourceHandle) -> &mut Self {
        self.mutations
            .push(TextResourceTransactionMutation::Remove { expected });
        self
    }

    pub fn mutations(&self) -> &[TextResourceTransactionMutation] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn apply(
        self,
        texts: &mut TextResourceArena,
        geometries: &GeometryResourceArena,
        fonts: &FontResourceArena,
    ) -> Result<TextResourceMutationTransactionResult, TextResourceMutationTransactionError> {
        self.preflight(texts, geometries, fonts)?;

        let mut applied = Vec::with_capacity(self.mutations.len());
        for mutation in self.mutations {
            match mutation {
                TextResourceTransactionMutation::Replace { expected, resource } => {
                    let current = replace_text_if_current(texts, expected, resource).expect(
                        "preflighted text resource replacement must remain valid while transaction owns the arena",
                    );
                    applied.push(AppliedTextResourceMutation::Replaced(current));
                }
                TextResourceTransactionMutation::Remove { expected } => {
                    remove_text_if_current(texts, expected).expect(
                        "preflighted text resource removal must remain valid while transaction owns the arena",
                    );
                    applied.push(AppliedTextResourceMutation::Removed(expected));
                }
            }
        }

        Ok(TextResourceMutationTransactionResult { applied })
    }

    fn preflight(
        &self,
        texts: &TextResourceArena,
        geometries: &GeometryResourceArena,
        fonts: &FontResourceArena,
    ) -> Result<(), TextResourceMutationTransactionError> {
        let mut targets = std::collections::BTreeSet::new();

        for (index, mutation) in self.mutations.iter().enumerate() {
            let expected = mutation.expected();
            if !targets.insert(expected.id) {
                return Err(TextResourceMutationTransactionError::DuplicateTarget {
                    index,
                    expected,
                });
            }

            let actual = texts.current_handle(expected.id).ok_or(
                TextResourceMutationTransactionError::Mutation {
                    index,
                    error: TextResourceMutationError::Resource(TextResourceError::UnknownResource(
                        expected.id,
                    )),
                },
            )?;
            if actual != expected {
                return Err(TextResourceMutationTransactionError::Mutation {
                    index,
                    error: TextResourceMutationError::Stale { expected, actual },
                });
            }
            if actual.version == u64::MAX {
                return Err(TextResourceMutationTransactionError::Mutation {
                    index,
                    error: TextResourceMutationError::Resource(TextResourceError::VersionExhausted(
                        actual.id,
                    )),
                });
            }

            let TextResourceTransactionMutation::Replace { resource, .. } = mutation else {
                continue;
            };
            resource.validate().map_err(|error| {
                TextResourceMutationTransactionError::Mutation {
                    index,
                    error: TextResourceMutationError::Resource(TextResourceError::InvalidResource(
                        error,
                    )),
                }
            })?;

            for vector in resource.vector_items.iter() {
                if geometries.get(vector.geometry).is_none() {
                    return Err(TextResourceMutationTransactionError::MissingGeometry {
                        index,
                        geometry: vector.geometry,
                    });
                }
            }
            for run in resource.runs.iter() {
                if fonts.get_for_face(&run.font).is_none() {
                    return Err(TextResourceMutationTransactionError::MissingFont {
                        index,
                        face: run.font.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppliedTextResourceMutation {
    Replaced(TextResourceHandle),
    Removed(TextResourceHandle),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextResourceMutationTransactionResult {
    applied: Vec<AppliedTextResourceMutation>,
}

impl TextResourceMutationTransactionResult {
    pub fn applied(&self) -> &[AppliedTextResourceMutation] {
        &self.applied
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextResourceMutationTransactionError {
    DuplicateTarget {
        index: usize,
        expected: TextResourceHandle,
    },
    Mutation {
        index: usize,
        error: TextResourceMutationError,
    },
    MissingGeometry {
        index: usize,
        geometry: GeometryResourceHandle,
    },
    MissingFont {
        index: usize,
        face: FontFaceIdentity,
    },
}

impl fmt::Display for TextResourceMutationTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTarget { index, expected } => write!(
                formatter,
                "text resource transaction mutation {index} repeats target {}@{}",
                expected.id.get(), expected.version
            ),
            Self::Mutation { index, error } => {
                write!(formatter, "text resource transaction mutation {index}: {error}")
            }
            Self::MissingGeometry { index, geometry } => write!(
                formatter,
                "text resource transaction mutation {index} references missing geometry {}@{}",
                geometry.id.get(), geometry.version
            ),
            Self::MissingFont { index, face } => write!(
                formatter,
                "text resource transaction mutation {index} references missing font {}#{}",
                face.face_key, face.face_index
            ),
        }
    }
}

impl std::error::Error for TextResourceMutationTransactionError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        FontResourceArena, GeometryId, GeometryResourceHandle, GlyphRun, Rect, TextAffineTransform,
        TextDirection, TextRenderItem, TextResourceValidationError, TextSourceKind,
        TextVectorItem, TextVectorStyle, Vec2,
    };

    use super::*;

    fn text(source: &str) -> TextResource {
        TextResource {
            source: Arc::from(source),
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
    fn multiple_replacements_commit_after_complete_preflight() {
        let mut texts = TextResourceArena::new();
        let first = texts.insert(text("a")).unwrap();
        let second = texts.insert(text("b")).unwrap();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let mut transaction = TextResourceMutationTransaction::new();
        transaction
            .replace(first, text("a2"))
            .replace(second, text("b2"));

        let result = transaction.apply(&mut texts, &geometries, &fonts).unwrap();
        let first_next = texts.current_handle(first.id).unwrap();
        let second_next = texts.current_handle(second.id).unwrap();
        assert_eq!(first_next.version, first.version + 1);
        assert_eq!(second_next.version, second.version + 1);
        assert_eq!(texts.get(first_next).unwrap().source.as_ref(), "a2");
        assert_eq!(texts.get(second_next).unwrap().source.as_ref(), "b2");
        assert_eq!(
            result.applied(),
            &[
                AppliedTextResourceMutation::Replaced(first_next),
                AppliedTextResourceMutation::Replaced(second_next),
            ]
        );
    }

    #[test]
    fn stale_late_mutation_prevents_earlier_valid_mutation() {
        let mut texts = TextResourceArena::new();
        let first = texts.insert(text("a")).unwrap();
        let second = texts.insert(text("b")).unwrap();
        let second_current = replace_text_if_current(&mut texts, second, text("b2")).unwrap();
        let before = texts.stats();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let mut transaction = TextResourceMutationTransaction::new();
        transaction
            .replace(first, text("should-not-commit"))
            .replace(second, text("stale"));

        assert_eq!(
            transaction.apply(&mut texts, &geometries, &fonts),
            Err(TextResourceMutationTransactionError::Mutation {
                index: 1,
                error: TextResourceMutationError::Stale {
                    expected: second,
                    actual: second_current,
                },
            })
        );
        assert_eq!(texts.current_handle(first.id), Some(first));
        assert_eq!(texts.get(first).unwrap().source.as_ref(), "a");
        assert_eq!(texts.current_handle(second.id), Some(second_current));
        assert_eq!(texts.stats(), before);
    }

    #[test]
    fn invalid_late_payload_prevents_earlier_valid_mutation() {
        let mut texts = TextResourceArena::new();
        let first = texts.insert(text("a")).unwrap();
        let second = texts.insert(text("b")).unwrap();
        let before = texts.stats();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let mut invalid = text("bad");
        invalid.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);
        let mut transaction = TextResourceMutationTransaction::new();
        transaction.replace(first, text("a2")).replace(second, invalid);

        assert_eq!(
            transaction.apply(&mut texts, &geometries, &fonts),
            Err(TextResourceMutationTransactionError::Mutation {
                index: 1,
                error: TextResourceMutationError::Resource(TextResourceError::InvalidResource(
                    TextResourceValidationError::InvalidRenderItem,
                )),
            })
        );
        assert_eq!(texts.current_handle(first.id), Some(first));
        assert_eq!(texts.current_handle(second.id), Some(second));
        assert_eq!(texts.stats(), before);
    }

    #[test]
    fn missing_geometry_dependency_prevents_all_mutation() {
        let mut texts = TextResourceArena::new();
        let first = texts.insert(text("a")).unwrap();
        let second = texts.insert(text("b")).unwrap();
        let before = texts.stats();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let missing = GeometryResourceHandle {
            id: GeometryId::new(42),
            version: 0,
        };
        let mut replacement = text("vector");
        replacement.vector_items = Arc::from([TextVectorItem {
            geometry: missing,
            transform: TextAffineTransform::IDENTITY,
            style: TextVectorStyle::default(),
            source_span: None,
            semantic_key: None,
        }]);
        replacement.render_items = Arc::from([TextRenderItem::Vector(0)]);
        let mut transaction = TextResourceMutationTransaction::new();
        transaction.replace(first, text("a2")).replace(second, replacement);

        assert_eq!(
            transaction.apply(&mut texts, &geometries, &fonts),
            Err(TextResourceMutationTransactionError::MissingGeometry {
                index: 1,
                geometry: missing,
            })
        );
        assert_eq!(texts.current_handle(first.id), Some(first));
        assert_eq!(texts.current_handle(second.id), Some(second));
        assert_eq!(texts.stats(), before);
    }

    #[test]
    fn missing_font_dependency_prevents_all_mutation() {
        let mut texts = TextResourceArena::new();
        let first = texts.insert(text("a")).unwrap();
        let second = texts.insert(text("b")).unwrap();
        let before = texts.stats();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let face = FontFaceIdentity {
            family: Arc::from("Missing Sans"),
            face_key: Arc::from("missing-sans-v1"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let mut replacement = text("font");
        replacement.runs = Arc::from([GlyphRun {
            font: face.clone(),
            variations: Arc::from([]),
            font_size: 48.0,
            direction: TextDirection::LeftToRight,
            fill: None,
            stroke: None,
            transform: TextAffineTransform::IDENTITY,
            glyphs: Arc::from([]),
        }]);
        replacement.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);
        let mut transaction = TextResourceMutationTransaction::new();
        transaction.replace(first, text("a2")).replace(second, replacement);

        assert_eq!(
            transaction.apply(&mut texts, &geometries, &fonts),
            Err(TextResourceMutationTransactionError::MissingFont { index: 1, face })
        );
        assert_eq!(texts.current_handle(first.id), Some(first));
        assert_eq!(texts.current_handle(second.id), Some(second));
        assert_eq!(texts.stats(), before);
    }

    #[test]
    fn duplicate_target_is_rejected_before_mutation() {
        let mut texts = TextResourceArena::new();
        let current = texts.insert(text("a")).unwrap();
        let before = texts.stats();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let mut transaction = TextResourceMutationTransaction::new();
        transaction
            .replace(current, text("a2"))
            .remove(current);

        assert_eq!(
            transaction.apply(&mut texts, &geometries, &fonts),
            Err(TextResourceMutationTransactionError::DuplicateTarget {
                index: 1,
                expected: current,
            })
        );
        assert_eq!(texts.current_handle(current.id), Some(current));
        assert_eq!(texts.stats(), before);
    }
}
