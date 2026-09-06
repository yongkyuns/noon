use noon_core::{
    MutationImpact, ObjectContentRef, ObjectId, Rect, Style, TrackDefinition, TrackId, Transform2D,
};

use crate::CompiledObject;

/// Renderer-independent mutations over the compiler-owned execution plan.
///
/// Content and object creation use the existing typed compiled representation.
/// The geometry-only external codec remains an explicit #959 legacy boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionPatch {
    CreateObject(CompiledObject),
    RemoveObject(ObjectId),
    SetContent {
        object: ObjectId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
    },
    SetTransform {
        object: ObjectId,
        transform: Transform2D,
    },
    SetStyle {
        object: ObjectId,
        style: Style,
    },
    AddTrack(TrackDefinition),
    ReplaceTrack(TrackDefinition),
    RemoveTrack(TrackId),
}

impl ExecutionPatch {
    pub const fn impact(&self) -> MutationImpact {
        match self {
            Self::SetContent { .. } | Self::SetTransform { .. } | Self::SetStyle { .. } => {
                MutationImpact::Property
            }
            Self::AddTrack(_) | Self::ReplaceTrack(_) | Self::RemoveTrack(_) => {
                MutationImpact::Timeline
            }
            Self::CreateObject(_) | Self::RemoveObject(_) => MutationImpact::Structure,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExecutionMutationTransaction {
    mutations: Vec<ExecutionPatch>,
}

impl ExecutionMutationTransaction {
    pub const fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    pub fn from_mutations(mutations: impl IntoIterator<Item = ExecutionPatch>) -> Self {
        Self {
            mutations: mutations.into_iter().collect(),
        }
    }

    pub fn push(&mut self, mutation: ExecutionPatch) {
        self.mutations.push(mutation);
    }

    pub fn mutations(&self) -> &[ExecutionPatch] {
        &self.mutations
    }

    pub fn is_empty(&self) -> bool {
        self.mutations.is_empty()
    }

    pub fn impact(&self) -> Option<MutationImpact> {
        self.mutations.iter().map(ExecutionPatch::impact).max()
    }
}
