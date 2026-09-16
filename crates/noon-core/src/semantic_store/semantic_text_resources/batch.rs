//! Cold glyph-text admission coupled to one existing semantic transaction.
use super::{SemanticStore, SemanticTextImportError};
use crate::{FontResourceArena, TextResource, TextResourceHandle};
use crate::{FontFaceIdentity, FontResourceKey, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult};
use std::{collections::BTreeMap, sync::Arc};

impl SemanticStore {
    /// Publish glyph-only text resources and one cold semantic transaction.
    ///
    /// All resource/font validation precedes insertion. Fonts are interned only
    /// after the atomic semantic transaction succeeds, under this same exclusive
    /// store borrow. A rejected builder/transaction removes fresh text resources
    /// generationally; existing resources and namespaces are untouched.
    ///
    /// This is deliberately NOT a live-execution publication callback: the
    /// builder receives only provisional text handles, not the store/runtime.
    /// Vector text requires the ordinary geometry-aware import path instead.
    pub fn apply_glyph_text_transaction<E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        build: impl FnOnce(&[TextResourceHandle]) -> Result<SemanticMutationTransaction, E>,
    ) -> Result<SemanticMutationTransactionResult, E>
    where
        E: From<SemanticTextImportError> + From<SemanticMutationTransactionError>
            + From<std::collections::TryReserveError>,
    {
        let mut fonts: BTreeMap<FontResourceKey, (FontFaceIdentity, Arc<[u8]>)> = BTreeMap::new();
        for (resource, source) in &inputs {
            resource.validate().map_err(SemanticTextImportError::Validation)?;
            if let Some(vector) = resource.vector_items.first() {
                return Err(SemanticTextImportError::MissingGeometry(vector.geometry).into());
            }
            for run in resource.runs.iter() {
                let key = FontResourceKey::from_face(&run.font);
                let incoming = source.get_for_face(&run.font)
                    .ok_or_else(|| SemanticTextImportError::MissingFont(key.clone()))?;
                let existing = self.font_resources.get_for_face(&run.font)
                    .map(|font| &font.data)
                    .or_else(|| fonts.get(&key).map(|(_, data)| data));
                if let Some(existing) = existing {
                    if !Arc::ptr_eq(existing, &incoming.data) && *existing != incoming.data {
                        return Err(SemanticTextImportError::Font(
                            crate::FontResourceError::ConflictingResource(key)).into());
                    }
                }
                // Check intra-batch conflicts even when the face is already in
                // the destination. No global font registry or arena copy.
                fonts.entry(key).or_insert_with(|| (run.font.clone(), incoming.data.clone()));
            }
        }
        let mut handles = Vec::new();
        handles.try_reserve_exact(inputs.len())?;
        for (resource, _) in inputs {
            handles.push(self.text_resources.insert(resource).expect("text batch preflighted"));
        }
        let result = build(&handles).and_then(|transaction| transaction.apply(self).map_err(E::from));
        if result.is_err() {
            for handle in handles {
                self.text_resources.remove(handle.id).expect("fresh unpublished text is removable");
            }
        } else {
            for (_, (face, data)) in fonts {
                self.font_resources.intern_face(&face, data).expect("font batch preflighted");
            }
        }
        result
    }
}
