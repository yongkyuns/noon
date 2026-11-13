//! Explicit low-level integration with Noon's existing authorities.
//!
//! Ordinary authoring uses [`Scene`](crate::Scene) and its authored handles.
//! This module exposes services for language wrappers, platform hosts,
//! resource adapters and diagnostics.
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
//! checks reject the resulting stale or foreign context. Use the owning `Scene`
//! for authoring, and `scene_execution` for host execution controls and publication.
//! If raw edits have already invalidated a session, explicitly discard/rebuild it
//! rather than overwriting its publication revision or treating old frames as new.
//!

#[cfg(feature = "code")]
pub mod code_input;
mod export_frames;
mod forward_sample;
mod frame_grid;
mod pixel_readback;
pub mod spatial_input;

pub use export_frames::{
    ExportAudioGrid, ExportAudioGridError, ExportEndReason, ExportFrame, ExportFrameOptions,
    ExportFramePolicy, ExportFramePolicyError, ExportFramePolicyStatus, ExportFrameSummary,
    ExportFrames, ExportFramesError, ExportFramesStatus, ExportSample, ExportSampleKind,
    ExportStop,
};
pub use forward_sample::{
    ForwardSample, ForwardSampleError, ForwardSampleStatus, SampleObservation,
};
pub use frame_grid::{FrameGrid, FrameGridError, FrameRate, FrameSample};
pub use pixel_readback::{
    PixelChannelOrder, PixelReadbackError, PixelRowOrder, Rgba8ReadbackLayout,
};

pub use crate::live_program::BlockingReplayError;
pub use crate::scene_script::{
    BlockingBoundary, BlockingContinuation, BlockingSceneHost, BlockingSceneSetup,
};

/// Prepare the bundled finite background score for a language host.
pub fn small_discovery_audio() -> Result<crate::AudioResource, AuthoringFailure> {
    use crate::example_scenes::background_audio::{music, SAMPLE_RATE};
    let pcm = music::render_music(SAMPLE_RATE)
        .map_err(|error| AuthoringFailure::unclassified("audio.music_preparation", &error))?;
    crate::AudioResource::from_interleaved(
        pcm.sample_rate(),
        music::CHANNELS as u16,
        pcm.into_samples(),
        crate::DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET,
    )
    .map_err(crate::AuthoringError::from)
    .map_err(Into::into)
}

/// Conversion from Manim Cairo pixel stroke widths to Noon scene units.
pub const MANIM_CAIRO_LINE_WIDTH_MULTIPLE: f64 = 0.01;

pub use crate::compact_value_authoring::semantic_object_state_from_compact;
pub use crate::execution_segment::{ExecutionSegmentSequence, ExecutionSegmentToken};
pub use crate::execution_session::{
    pointer_selection_overlay_color, PointerFillOutcome, PointerFillQuery, PointerFillUnsupported,
    PointerSelectionClick, PointerSelectionHighlight, PointerSelectionPresentation,
};
pub use crate::execution_session::{
    CallbackAdvance, CallbackPhaseOverlay, CallbackPhaseToken, CallbackReadRequest,
    CallbackReadValue, CallbackRegionAdvance, CallbackRendererDirtyClassification,
    CallbackRendererObservationOutcome, CallbackSequence, CallbackTermination,
    CallbackTerminationKind, CommittedCallbackRendererObservation, EffectivePropertyBatch,
    EffectiveSemanticObject, EffectiveSemanticPropertyWrite, ExecutionViewportQuery,
    RequiredCallbackInvocation, StaleViewportQuery, StructuralPublicationStats,
};
pub use crate::execution_session::{NativePointerInputPublication, NativePointerInputToken};
pub use crate::execution_session::{PointerFrameError, PointerFrameSnapshot, PointerFrameView};
pub use crate::host_callbacks::{
    effective_style_with_color, effective_style_with_fade, effective_style_with_fade_to,
    effective_style_with_fill, effective_style_with_fill_color, effective_style_with_fill_opacity,
    effective_style_with_paint_opacity, effective_style_with_stroke_color,
    rotate_effective_transform_about_point,
};
pub use crate::scene::host_execution as scene_execution;
pub use crate::semantic_mobject::{authoring_render_f64, authoring_xy_f64, line_match_transform};
#[cfg(feature = "native-text")]
pub use crate::text_authoring::NATIVE_POINT_TO_SCENE_SCALE;
#[cfg(feature = "typst")]
pub use crate::text_authoring::SCALE_FACTOR_PER_FONT_POINT;
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
pub use noon_core::{
    NativeInputModifiers, NativePointerCancellation, NativePointerContext, NativePointerId,
    NativePointerInput, NativePointerInputKind, NativePointerPosition,
};
pub use noon_runtime::{
    AudioMixError, AudioMixFormat, EffectiveObjectProperties, FrameChanges, FrameObjectState,
    FrameState, PreparedAudioMix, RendererPublication, ReplayError, ReplayLimits, RuntimeIdentity,
    RuntimeWakeState, TimelineWakeState, MAX_AUDIO_MIX_BLOCK_FRAMES,
};

mod vector_path;
pub use crate::binding_error::AuthoringFailure;

/// Preserve typed live resource and compiler diagnostics at the language boundary.
#[cfg(any(feature = "native-text", feature = "typst", feature = "latex"))]
pub fn live_text_failure(error: crate::TextAuthoringError) -> AuthoringFailure {
    match error {
        crate::TextAuthoringError::Semantic(crate::AuthoringError::ExecutionPublication(error)) => {
            crate::LiveSessionError::Publication(error)
        }
        other => crate::LiveSessionError::Text(other),
    }
    .into()
}

/// Preserve the language host's live numeric errors when Scene owns publication.
#[cfg(feature = "latex")]
pub fn live_numeric_failure(error: crate::NumericAuthoringError) -> AuthoringFailure {
    use crate::{AuthoringError, LiveSessionError, NumericAuthoringError, TextAuthoringError};
    match error {
        NumericAuthoringError::Text(TextAuthoringError::Semantic(
            AuthoringError::ExecutionPublication(error),
        )) => LiveSessionError::Publication(error),
        NumericAuthoringError::Text(error) => LiveSessionError::Text(error),
        NumericAuthoringError::Semantic(error) => error.into(),
        other => LiveSessionError::Mobject(other.to_string()),
    }
    .into()
}
/// Preserve the language host's live Variable construction errors.
#[cfg(feature = "latex")]
pub fn live_variable_failure(error: crate::VariableAuthoringError) -> AuthoringFailure {
    use crate::{LiveSessionError, VariableAuthoringError};
    match error {
        VariableAuthoringError::Text(error) => LiveSessionError::Text(error).into(),
        VariableAuthoringError::Numeric(error) => live_numeric_failure(error),
        VariableAuthoringError::Authoring(error) => LiveSessionError::from(error).into(),
    }
}
pub use crate::binding_options::*;
pub use vector_path::{authoring_path_point, AuthoringVectorPath};

mod image_options;
pub use image_options::image_sampling;
#[cfg(feature = "native-text")]
mod text_options;
#[cfg(feature = "native-text")]
pub use text_options::{native_text_options, text_color_channel};

pub fn family_color(
    enabled: bool,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<Option<crate::Color>, String> {
    if !enabled {
        return Ok(None);
    }
    if ![red, green, blue, alpha]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return Err("family color components must be finite and between zero and one".into());
    }
    Ok(Some(crate::Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    )))
}

mod effect_options;
pub use effect_options::{effect_color, effect_update, mask_parameters};

mod paint_options;
pub use paint_options::{
    hatch_paint, image_mapping, linear_gradient, noise_paint, radial_gradient,
};

mod plot_error;
pub use plot_error::{
    coordinate_failure, coordinate_math_failure, live_coordinate_failure,
    live_coordinate_publication_failure, plot_failure, sampling_failure,
};
mod plot_input;
pub use plot_input::{data_points as plot_data_points, PlotSamplingInput};

/// Decode the optional frontend spelling of an existing camera working profile.
pub fn camera_color_profile(value: &str) -> Result<noon_core::ViewColorProfile, AuthoringFailure> {
    match value {
        "compatibility" => Ok(noon_core::ViewColorProfile::Compatibility),
        "linear_hdr" => Ok(noon_core::ViewColorProfile::LinearHdr),
        _ => Err(AuthoringFailure::new(
            "invalid_input",
            "camera.color_profile",
            "view color profile must be compatibility or linear_hdr",
        )),
    }
}

mod style_input;
pub use style_input::{style_update, style_update_with_background_stroke};
