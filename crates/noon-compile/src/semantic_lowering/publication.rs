//! Local value lowering for an exclusively prepared authored transaction.
use std::collections::HashMap;

use noon_core::{
    MutationTransaction, PreparedSemanticMutationTransaction, ScenePatch, SemanticMutation,
    SemanticMutationTransaction, SemanticNodeId, SemanticObjectProperty,
};

use super::{
    projection::{lower_semantic_style, lower_semantic_transform},
    SemanticExecutionIndex, SemanticLoweringError,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticPublicationLoweringError {
    UnsupportedMutation { index: usize },
    Value(SemanticLoweringError),
}

impl std::fmt::Display for SemanticPublicationLoweringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedMutation { index } => write!(
                f,
                "semantic mutation {index} has no incremental live publication contract"
            ),
            Self::Value(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for SemanticPublicationLoweringError {}
impl From<SemanticLoweringError> for SemanticPublicationLoweringError {
    fn from(error: SemanticLoweringError) -> Self {
        Self::Value(error)
    }
}

/// Reject unsupported work before semantic preflight can traverse topology.
pub fn validate_semantic_publication(
    transaction: &SemanticMutationTransaction,
) -> Result<(), SemanticPublicationLoweringError> {
    validate_mutations(transaction.mutations())
}

fn validate_mutations(
    mutations: &[SemanticMutation],
) -> Result<(), SemanticPublicationLoweringError> {
    for (position, mutation) in mutations.iter().enumerate() {
        if !matches!(
            mutation,
            SemanticMutation::SetProperty { .. }
                | SemanticMutation::ReplaceStyle { .. }
                | SemanticMutation::AddNode { .. }
                | SemanticMutation::AddAnimation { .. }
        ) {
            return Err(SemanticPublicationLoweringError::UnsupportedMutation { index: position });
        }
    }
    Ok(())
}

/// Lower only changed transform/style values of objects in this execution domain.
///
/// Detached declarations and detached object values remain authored-only until a
/// subsequent initial scene lowering includes them. They still publish the store's
/// revision; this helper never changes membership or silently rebuilds a session.
/// Resource, topology, signal and subscription changes fail closed until their
/// corresponding incremental preparation contracts are available.
pub fn lower_semantic_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    index: &SemanticExecutionIndex,
) -> Result<MutationTransaction, SemanticPublicationLoweringError> {
    validate_mutations(prepared.mutations())?;
    let mut domains: HashMap<SemanticNodeId, (bool, bool)> = HashMap::new();
    for mutation in prepared.candidate_mutations() {
        match mutation {
            SemanticMutation::SetProperty {
                object, property, ..
            } => {
                let flags = domains.entry(*object).or_default();
                match property {
                    SemanticObjectProperty::Translation
                    | SemanticObjectProperty::Scale
                    | SemanticObjectProperty::RotationZ => flags.0 = true,
                    _ => flags.1 = true,
                }
            }
            SemanticMutation::ReplaceStyle { object, .. } => {
                domains.entry(*object).or_default().1 = true
            }
            SemanticMutation::AddNode { .. } | SemanticMutation::AddAnimation { .. } => {}
            _ => unreachable!("supported vocabulary checked above"),
        }
    }
    let mut mutations = Vec::with_capacity(domains.len() * 2);
    for (node, state) in prepared.object_updates() {
        let Some(object) = index.execution_object_id(node) else {
            continue;
        };
        let (transform, style) = domains[&node];
        if transform {
            mutations.push(ScenePatch::SetTransform {
                object,
                transform: lower_semantic_transform(node, &state)?,
            });
        }
        if style {
            mutations.push(ScenePatch::SetStyle {
                object,
                style: lower_semantic_style(node, &state)?,
            });
        }
    }
    Ok(MutationTransaction::from_mutations(mutations))
}
