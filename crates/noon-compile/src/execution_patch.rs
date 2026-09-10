use noon_core::{
    ObjectContentRef, ObjectId, Property, Rect, Style, TrackDefinition, TrackId, Transform2D,
};

use crate::{CompiledFamilyAnimation, CompiledObject};

/// Renderer-independent mutations over the compiler-owned execution plan.
///
/// Content and object creation use the existing typed compiled representation.
#[derive(Clone, Debug, PartialEq)]
pub enum ExecutionPatch {
    CreateObject(CompiledObject),
    RemoveObject(ObjectId),
    /// Move one live object in the derived family traversal order without relocating its
    /// stable execution row. Z-index determines painter layers; equal layers follow
    /// this traversal. `before=None` moves it to the live family tail.
    ReorderObject {
        object: ObjectId,
        before: Option<ObjectId>,
    },
    SetContent {
        object: ObjectId,
        content: ObjectContentRef,
        text_bounds: Option<Rect>,
    },
    SetTransform {
        object: ObjectId,
        transform: Transform2D,
    },
    SetZIndex {
        object: ObjectId,
        value: f64,
    },
    SetStyle {
        object: ObjectId,
        style: Style,
    },
    AddTrack(TrackDefinition),
    AddFamilyAnimation(CompiledFamilyAnimation),
    ReplaceTrack(TrackDefinition),
    RemoveTrack(TrackId),
    /// Release one completed timeline driver while retaining its deterministic history.
    ReconcileTrack {
        track: TrackId,
        object: ObjectId,
        property: Property,
        end_time: f64,
    },
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
}
