use noon_core::{
    GraphEdgeId, ObjectContentRef, ObjectId, Property, Rect, SemanticTransform, Style,
    TrackDefinition, TrackId, Transform2D,
};

use crate::{
    CompiledFamilyAnimation, CompiledGraphArrowPolicy, CompiledObject, CompiledScene,
    CompiledSpatialState,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompiledGraphDependencyKind {
    Line,
    Arrow {
        end_tip: ObjectId,
        start_tip: Option<ObjectId>,
        policy: CompiledGraphArrowPolicy,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompiledGraphDependencyDefinition {
    pub edge: GraphEdgeId,
    pub start_vertex: ObjectId,
    pub end_vertex: ObjectId,
    pub line: ObjectId,
    pub kind: CompiledGraphDependencyKind,
}

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
    /// Replace the complete authored transform while preserving spatial values in
    /// the compiler-owned f64 column.
    SetSemanticTransform {
        object: ObjectId,
        transform: SemanticTransform,
    },
    /// Commit a prepared transform together with its optional spatial routing
    /// metadata. Used when a semantic domain edit promotes/demotes one row.
    SetSpatialState {
        object: ObjectId,
        base_transform: Transform2D,
        spatial: Option<CompiledSpatialState>,
    },
    SetZIndex {
        object: ObjectId,
        value: f64,
    },
    SetStyle {
        object: ObjectId,
        style: Style,
    },
    /// Replace one graph root's complete endpoint dependency declaration.
    /// Empty dependencies retire that root without relocating unrelated slots.
    SetGraphDependencies {
        owner: ObjectId,
        dependencies: Vec<CompiledGraphDependencyDefinition>,
    },
    /// Replace the affected anchor group's leaf rows after one prepared family
    /// membership edit. IDs are semantic leaf identities encoded as execution keys.
    SetFixedOrientationGroupBoundsMembers {
        anchor_family: noon_core::SemanticNodeId,
        members: Vec<ObjectId>,
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

impl CompiledScene {
    /// Precompute the compact and optional spatial representation of one authored
    /// transform for an already-identified object slot.
    pub fn prepare_semantic_transform_value(
        &self,
        object_index: u32,
        transform: SemanticTransform,
    ) -> Option<(Transform2D, Option<crate::CompiledSpatialState>)> {
        let object = self.objects().get(object_index as usize)?;
        super::lower_semantic_transform_patch(object.id, object.spatial.as_deref(), transform)
    }

    /// Commit a semantic transform computed during prepared publication.
    #[doc(hidden)]
    pub fn commit_prepared_semantic_transform_value(
        &mut self,
        object_index: u32,
        base_transform: Transform2D,
        spatial: Option<crate::CompiledSpatialState>,
    ) {
        let previous = self.objects[object_index as usize]
            .spatial
            .as_deref()
            .cloned();
        self.update_fixed_orientation_group(object_index, previous.as_ref(), spatial.as_ref());
        let object = &mut self.objects[object_index as usize];
        object.base_transform = base_transform;
        match spatial {
            Some(next) => {
                if let Some(current) = object.spatial.as_deref_mut() {
                    *current = next;
                } else {
                    object.spatial = Some(Box::new(next));
                }
            }
            None => object.spatial = None,
        }
    }

    /// Commit one transform value whose object slot and numeric payload were already
    /// validated by the caller's prepared publication proof.
    ///
    /// This is deliberately narrower than `apply_execution_patch`: it performs no
    /// lookup or validation and cannot publish structural/timeline/resource edits.
    #[doc(hidden)]
    pub fn commit_prepared_transform_value(&mut self, object_index: u32, transform: Transform2D) {
        self.objects[object_index as usize].base_transform = transform;
    }

    /// Commit one style value whose object slot and payload were already validated
    /// by the caller's prepared publication proof.
    #[doc(hidden)]
    pub fn commit_prepared_style_value(&mut self, object_index: u32, style: Style) {
        self.objects[object_index as usize].base_style = style;
    }
}
