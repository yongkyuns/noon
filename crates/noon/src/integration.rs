//! Explicit low-level integration with Noon's existing authorities.
//!
//! Ordinary authoring uses [`Scene`](crate::Scene), [`Mobject`](crate::Mobject),
//! and [`LiveSession`](crate::LiveSession). This module exposes only the specific
//! types needed by language wrappers, platform hosts, resource adapters and
//! diagnostics; it owns no scene, runtime, registry or scheduler.
//!
//! # Raw semantic access
//!
//! [`Scene::with_integration_store`](crate::Scene::with_integration_store) accepts
//! an existing arena. The `integration_store()` accessors on Scene, Mobject and
//! MobjectFamily return the same arena, not a snapshot. Keep RefCell borrows short
//! and release them before calling authoring/session operations.
//!
//! Mutating the arena (including through an authored handle) after lowering does
//! **not** publish that change into a live session. Existing revision and identity
//! checks reject the resulting stale or foreign context. They are never disabled
//! by using this module. Prefer `scene.live(&mut session)` for live mutation; raw
//! callers must use the session's coherent semantic-transaction publication API.
//! If raw edits have already invalidated a session, explicitly discard/rebuild it
//! rather than overwriting its publication revision or treating old frames as new.
//!
//! The retained text adapter below is still needed by explicit transport callers
//! and is deletion-owned by #959. It is not the ordinary Scene authoring API.

pub use crate::boolean_authoring::effective_boolean_geometry_options;
pub use crate::compact_value_authoring::semantic_object_state_from_compact;
pub use crate::execution_segment::{ExecutionSegmentSequence, ExecutionSegmentToken};
pub use crate::execution_session::{
    CallbackAdvance, CallbackPhaseOverlay, CallbackPhaseToken, CallbackReadRequest,
    CallbackReadValue, CallbackRendererDirtyClassification, CallbackRendererObservationOutcome,
    CallbackSequence, CallbackTermination, CallbackTerminationKind,
    CommittedCallbackRendererObservation, EffectivePropertyBatch, EffectiveSemanticObject,
    EffectiveSemanticPropertyWrite, ExecutionViewportQuery, RequiredCallbackInvocation,
    StructuralPublicationStats,
};
pub use crate::host_callbacks::{
    effective_style_with_color, effective_style_with_fill, effective_style_with_fill_color,
    effective_style_with_fill_opacity, effective_style_with_paint_opacity,
    effective_style_with_stroke_color, rotate_effective_transform_about_point,
};
pub use crate::path_alignment::publish_alignment;
pub use crate::path_queries::effective_path_query;
pub use crate::point_matching::publish_match_points;
pub use crate::semantic_mobject::{authoring_render_f64, authoring_xy_f64, line_match_transform};
#[cfg(feature = "native-text")]
pub use crate::text_authoring::NATIVE_POINT_TO_SCENE_SCALE;
#[cfg(feature = "typst")]
pub use crate::text_authoring::SCALE_FACTOR_PER_FONT_POINT;
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use crate::text_authoring::{RetainedMobject, RetainedScene};
pub use crate::z_index::{effective_z_index, publish_z_index};
pub use noon_core::{
    GeometryResource, GeometryResourceArena, GeometryResourceError, GeometryResourceHandle,
    GeometryResourceLookup, HostCallbackId, SemanticFamilyPairingError, SemanticLoweringError,
    SemanticMutationImpact, SemanticMutationTransaction, SemanticMutationTransactionError,
    SemanticMutationTransactionResult, SemanticNodeCreation, SemanticNodeKind,
    SemanticScalarSignalQueryError, SemanticSceneOperationError, SemanticSignalBindingError,
    SemanticSignalError, SemanticStore, SemanticStoreError, SemanticStoreIdentity,
    SemanticTextImportError, SemanticTransactionNodeRef, TextResource, TextResourceArena,
    TextResourceHandle,
};
pub use noon_runtime::{
    EffectiveObjectProperties, FrameChanges, FrameObjectState, FrameState, RendererPublication,
    RuntimeIdentity, RuntimeWakeState, TimelineWakeState,
};

/// Publish a family translation through the existing coherent execution authority.
pub fn publish_family_shift(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    x: f64,
    y: f64,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_shift_family(store, root, execution, family, x, y)
}

/// Publish linear family arrangement without constructing a `LiveSession` facade.
pub fn publish_family_arrangement(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    options: &crate::FamilyArrangeOptions,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_arrange_family(store, root, execution, family, options)
}

/// Publish grid family arrangement without constructing a `LiveSession` facade.
pub fn publish_family_grid_arrangement(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    options: &crate::FamilyGridOptions,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_arrange_family_in_grid(store, root, execution, family, options)
}

/// Publish object placement from one coherent effective layout observation.
pub fn publish_mobject_move_to(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    object: &crate::Mobject,
    target: crate::LiveLayoutTarget<'_>,
    edge: (f64, f64),
    mask: (f64, f64),
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_move_to(store, root, execution, object, target, edge, mask)
}

/// Publish family placement from one coherent effective layout observation.
pub fn publish_family_move_to(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    target: crate::LiveLayoutTarget<'_>,
    edge: (f64, f64),
    mask: (f64, f64),
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_move_family_to(store, root, execution, family, target, edge, mask)
}

/// Publish family next-to placement from one coherent effective layout observation.
pub fn publish_family_next_to(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    target: crate::LiveLayoutTarget<'_>,
    args: crate::ManimNextToArgs,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_next_family_to(store, root, execution, family, target, args)
}

/// Publish selected-layout next-to placement with a distinct effective aligner.
pub fn publish_layout_next_to_aligned(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    source: &crate::LayoutAnchor,
    target: crate::LiveLayoutTarget<'_>,
    aligner: &crate::LayoutAnchor,
    args: crate::ManimNextToArgs,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_next_layout_to_aligned(
        store, root, execution, source, target, aligner, args,
    )
}

/// Publish family alignment to the default frame.
pub fn publish_family_align_on_frame(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    direction: (f64, f64),
    buff: f64,
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_align_family_on_frame(
        store, root, execution, family, direction, buff,
    )
}

/// Publish family alignment against another effective target.
pub fn publish_family_align_to(
    store: &std::rc::Rc<std::cell::RefCell<SemanticStore>>,
    root: crate::SemanticNodeId,
    execution: &mut crate::ExecutionSession,
    family: &crate::MobjectFamily,
    target: crate::LiveLayoutTarget<'_>,
    axis: (f64, f64),
) -> Result<SemanticMutationTransactionResult, crate::AuthoringError> {
    crate::family_layout::publish_align_family_to(store, root, execution, family, target, axis)
}
