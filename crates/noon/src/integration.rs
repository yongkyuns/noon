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
    effective_style_with_fill_opacity, effective_style_with_stroke_color,
    rotate_effective_transform_about_point,
};
pub use crate::semantic_mobject::{authoring_render_f64, authoring_xy_f64, line_match_transform};
#[cfg(feature = "native-text")]
pub use crate::text_authoring::NATIVE_POINT_TO_SCENE_SCALE;
#[cfg(feature = "typst")]
pub use crate::text_authoring::SCALE_FACTOR_PER_FONT_POINT;
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use crate::text_authoring::{RetainedMobject, RetainedScene};
pub use noon_core::{
    GeometryResource, GeometryResourceArena, GeometryResourceError, GeometryResourceHandle,
    GeometryResourceLookup, HostCallbackId, SemanticFamilyPairingError,
    SemanticGeometryLayoutError, SemanticLoweringError, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticMutationTransactionError,
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
