use noon_core::{
    GraphEdgeId, ObjectContentRef, ObjectId, Property, Rect, SemanticTransform, Style,
    TrackDefinition, TrackId, Transform2D,
};

use crate::{
    CompiledFamilyAnimation, CompiledGraphArrowPolicy, CompiledObject, CompiledScene,
    CompiledSpatialState,
};

/// Validate the finite continuous-paint execution profile at a genuine external
/// boundary as well as typed execution-plan publication.
pub fn validate_fill_paint(
    object: ObjectId,
    gradient: Option<&noon_core::SemanticPaint>,
) -> Result<(), crate::CompilePatchError> {
    if let Some(gradient) = gradient {
        crate::semantic_lowering::paint::validate_definition(gradient).map_err(|_| {
            crate::CompilePatchError::InvalidObjectState {
                object,
                field: noon_core::ObjectStateField::Style,
            }
        })?;
    }
    Ok(())
}

/// Sparse lowered relation between ordinary rectangle rows. Geometry and pose
/// remain owned by those rows; this contains no separately evaluated view state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompiledInset2DView {
    pub camera_frame: ObjectId,
    pub capture_own_display: bool,
}

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
    /// Root drawing eligibility; does not change semantic query geometry.
    SetDrawEnabled {
        object: ObjectId,
        enabled: bool,
    },
    SetStyle {
        object: ObjectId,
        style: Style,
        fill_paint: Option<std::sync::Arc<noon_core::SemanticPaint>>,
    },
    /// Replace one display's relation. The camera may be detached; the relation
    /// becomes visible only while both execution rows are live.
    SetInset2DView {
        object: ObjectId,
        view: Option<CompiledInset2DView>,
    },
    /// Replace one immutable parameter column. The complete ordered identity
    /// list and every surviving kind/unit/source schema must remain unchanged.
    SetEffects {
        object: ObjectId,
        effects: std::sync::Arc<crate::CompiledEffectStack>,
    },
    /// Replace ordered attachment topology after checking the complete identity
    /// list. Retire only removed generations; value-only edits use SetEffects.
    SetEffectStack {
        object: ObjectId,
        expected: Box<[noon_core::SemanticNodeId]>,
        effects: Option<std::sync::Arc<crate::CompiledEffectStack>>,
    },
    /// Replace one graph root's complete endpoint dependency declaration.
    /// Empty dependencies retire that root without relocating unrelated slots.
    SetGraphDependencies {
        owner: ObjectId,
        dependencies: Vec<CompiledGraphDependencyDefinition>,
    },
    /// Replace the affected anchor group's leaf rows after one prepared family
    /// membership edit. IDs are semantic leaf identities encoded as execution keys.
    SetSpatialAnchorGroupBoundsMembers {
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
        spatial: Option<Box<crate::CompiledSpatialState>>,
    ) {
        let previous = self.objects[object_index as usize]
            .spatial
            .as_deref()
            .cloned();
        self.update_spatial_anchor_group(object_index, previous.as_ref(), spatial.as_deref());
        let object = &mut self.objects[object_index as usize];
        object.base_transform = base_transform;
        match spatial {
            Some(next) => {
                if let Some(current) = object.spatial.as_deref_mut() {
                    *current = *next;
                } else {
                    object.spatial = Some(next);
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
    pub fn commit_prepared_style_value(
        &mut self,
        object_index: u32,
        style: Style,
        fill_paint: Option<std::sync::Arc<noon_core::SemanticPaint>>,
    ) {
        let object = &mut self.objects[object_index as usize];
        object.base_style = style;
        object.fill_paint = fill_paint;
    }
}

impl CompiledScene {
    /// Commit an already-preflighted existing attachment value on one stable row.
    #[doc(hidden)]
    pub fn commit_prepared_effect_values(
        &mut self,
        object_index: u32,
        effects: std::sync::Arc<crate::CompiledEffectStack>,
    ) {
        self.objects[object_index as usize].effects = Some(effects);
    }
}

/// Value updates must not rebind compiled track endpoints to another generation
/// or silently reinterpret radius/source channels. Definition fields are validated
/// by each effect's typed construction before this boundary. Discrete fields
/// without a driver may have a checked persistent-update policy distinct from
/// interpolation (for example, a dissolve seed).
pub(crate) fn validate_effect_values(
    object: ObjectId,
    previous: Option<&crate::CompiledEffectStack>,
    next: &crate::CompiledEffectStack,
) -> Result<(), crate::CompilePatchError> {
    let previous = previous.ok_or(crate::CompilePatchError::InvalidEffectUpdate { object })?;
    if previous.attachments() != next.attachments() {
        return Err(crate::CompilePatchError::InvalidEffectUpdate { object });
    }
    for (old, new) in previous.iter().zip(next.iter()) {
        noon_core::EffectValue::validate_effect_value_update(old.definition, new.definition)
            .map_err(|_| crate::CompilePatchError::InvalidEffectUpdate { object })?;
    }
    Ok(())
}
