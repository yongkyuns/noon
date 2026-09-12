//! Borrowed live access to one already-published execution session.
//!
//! This facade owns neither semantic nor runtime state.  It only coordinates a
//! transaction with the session that already lowered the same semantic store.
//! Membership and property publication use the same prepared semantic transaction.
//! Existing affine declarations use session-local segments, whose endpoint
//! reconciliation remains owned by `ExecutionSession::complete_segment`.

mod boolean_geometry;
mod dashed_vmobject;
mod family_layout;
mod path_alignment;
mod path_editing;
mod path_queries;
mod point_matching;
mod z_index;
pub use family_layout::LiveLayoutTarget;

use crate::execution_session::EffectiveSemanticObject;
use crate::{
    family_arrangement::FamilyArrangePlan,
    semantic_mobject::{authoring_render_f64, stage_state_changes},
    semantic_mobject::{
        edit_color, edit_disable_fill, edit_disable_stroke, edit_fill, edit_fill_color,
        edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke, edit_stroke_color,
        edit_stroke_opacity,
    },
    state_replacement::prepare_become_state,
    DeclaredAnimation, ExecutionSegment, ExecutionSegmentAdvanceError,
    ExecutionSegmentCompletionError, ExecutionSegmentError, ExecutionSegmentState,
    ExecutionSession, ExecutionSessionAnimationError, ExecutionSessionPublicationError,
    ManimBecomeOptions, ManimLineEndpoints, Mobject, MobjectFamily, MobjectFamilyMember,
    SceneMembershipRequest, ValueTracker,
};
use noon_core::{
    AnimationOptions, Bounds2D64, Color, PublicationContext, SemanticAffineLifecycleDirection,
    SemanticAffineLifecycleEndpoint, SemanticAnimationCompositionKind, SemanticFadeDirection,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticObjectProperty, SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticStyle,
    SemanticSubsetDisplayMode, SemanticVec3, Style, Transform2D,
};
use std::{cell::RefCell, rc::Rc};

/// An owned observation of one effective runtime object at one publication.
///
/// The clone makes the observation safe to retain after the next live mutation;
/// it is an observation, not another runtime authority.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveMobjectState {
    pub z_index: f64,
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub publication: PublicationContext,
}

impl EffectiveMobjectState {
    /// Current fill alpha, excluding the separate object-composite multiplier.
    pub fn fill_opacity(&self) -> f64 {
        self.style.fill.map_or(0.0, |color| f64::from(color.alpha))
    }

    /// Current stroke alpha, excluding the separate object-composite multiplier.
    pub fn stroke_opacity(&self) -> f64 {
        self.style
            .stroke
            .map_or(0.0, |color| f64::from(color.alpha))
    }
}

/// One object's exact layout observation at a coherent runtime publication.
///
/// These bounds retain authored layout semantics and therefore exclude the
/// renderer's conservative stroke expansion used for visibility indexing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectiveMobjectLayout {
    pub center: (f64, f64),
    pub width: f64,
    pub height: f64,
    pub publication: PublicationContext,
}

/// Activation-relative endpoint for one shared affine appearance lifecycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AffineLifecycleEndpoint {
    Point {
        x: f64,
        y: f64,
        rotation_offset: f64,
        point_color: Option<Color>,
    },
    /// Resolve the target's effective layout center at the activation publication.
    EffectiveCenter,
}

pub type AffineLifecycleDirection = SemanticAffineLifecycleDirection;
pub type SubsetDisplayMode = SemanticSubsetDisplayMode;

/// Outline style and local phase easing for shared DrawBorderThenFill semantics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawBorderThenFillOptions {
    pub stroke_width: f64,
    pub stroke_color: Option<Color>,
    pub phase_rate_function: noon_core::RateFunction,
}

impl DrawBorderThenFillOptions {
    pub const fn new(stroke_width: f64, stroke_color: Option<Color>) -> Self {
        Self {
            stroke_width,
            stroke_color,
            phase_rate_function: noon_core::RateFunction::Smooth,
        }
    }

    pub const fn with_phase_rate_function(
        mut self,
        phase_rate_function: noon_core::RateFunction,
    ) -> Self {
        self.phase_rate_function = phase_rate_function;
        self
    }
}

impl Default for DrawBorderThenFillOptions {
    fn default() -> Self {
        Self::new(0.02, None)
    }
}

/// Placement of the faded affine endpoint relative to activation-effective layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FadeTranslation {
    Shift(SemanticVec3),
    Point(SemanticVec3),
}

/// Scale and placement applied to the faded copy of one canonical object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeEndpoint {
    pub scale_factor: f64,
    pub translation: FadeTranslation,
}

impl FadeEndpoint {
    pub const fn new(scale_factor: f64, translation: FadeTranslation) -> Self {
        Self {
            scale_factor,
            translation,
        }
    }
}

impl Default for FadeEndpoint {
    fn default() -> Self {
        Self::new(1.0, FadeTranslation::Shift(SemanticVec3::ZERO))
    }
}

/// Appearance endpoint for the shared restoring Indicate composition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IndicateOptions {
    pub scale_factor: f64,
    pub color: Color,
}

impl IndicateOptions {
    pub const fn new(scale_factor: f64, color: Color) -> Self {
        Self {
            scale_factor,
            color,
        }
    }
}

impl Default for IndicateOptions {
    fn default() -> Self {
        Self::new(1.2, noon_core::YELLOW)
    }
}

/// Reconstruct the supported authored target style directly from one effective
/// runtime row. Preserve exact authored values when their lowered values match.
/// Changed runtime colors already contain evaluated paint opacity, so capture
/// those as solid paints with unit paint opacity. Resource paints are not
/// represented by `Style` and must remain explicitly unavailable here.
pub(crate) fn target_style_from_effective(
    authored: &SemanticStyle,
    effective: Style,
) -> Result<SemanticStyle, LiveSessionError> {
    if matches!(
        authored.fill.as_ref(),
        Some(noon_core::SemanticPaint::Resource(_))
    ) || matches!(
        authored.stroke.as_ref(),
        Some(noon_core::SemanticPaint::Resource(_))
    ) {
        return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::CaptureResourcePaint,
        )));
    }
    let (fill, fill_opacity) =
        if lowered_solid_color(authored.fill.as_ref(), authored.fill_opacity) == effective.fill {
            (authored.fill.clone(), authored.fill_opacity)
        } else {
            (effective.fill.map(noon_core::SemanticPaint::Solid), 1.0)
        };
    let (stroke, stroke_opacity) =
        if lowered_solid_color(authored.stroke.as_ref(), authored.stroke_opacity)
            == effective.stroke
        {
            (authored.stroke.clone(), authored.stroke_opacity)
        } else {
            (effective.stroke.map(noon_core::SemanticPaint::Solid), 1.0)
        };
    Ok(SemanticStyle {
        fill,
        fill_opacity,
        stroke,
        stroke_opacity,
        // Retain authored precision when runtime lowering did not change width.
        // An f32 round trip must not invent a structural style change.
        stroke_width: if authored.stroke_width as f32 == effective.stroke_width {
            authored.stroke_width
        } else {
            f64::from(effective.stroke_width)
        },
        stroke_width_mode: effective.stroke_width_mode,
        stroke_join: effective.stroke_join,
        stroke_cap: effective.stroke_cap,
        object_opacity: if authored.object_opacity as f32 == effective.opacity {
            authored.object_opacity
        } else {
            f64::from(effective.opacity)
        },
    })
}

fn lowered_solid_color(paint: Option<&noon_core::SemanticPaint>, opacity: f64) -> Option<Color> {
    let noon_core::SemanticPaint::Solid(color) = paint? else {
        return None;
    };
    Some(Color {
        alpha: (f64::from(color.alpha) * f64::from(opacity as f32)) as f32,
        ..*color
    })
}

fn preserve_or_capture_f32(authored: &mut f64, effective: f32) {
    if *authored as f32 != effective {
        *authored = f64::from(effective);
    }
}

/// One borrowed TransformTo leaf in an atomic live composition request.
///
/// This value contains no schedule or runtime state. The shared Rust compiler resolves all child
/// intervals and captures effective properties when the request is consumed.
#[derive(Clone, Copy)]
pub struct TransformToRequest<'a> {
    source: &'a Mobject,
    target_state: &'a Mobject,
    interpolation: noon_core::SemanticTransformInterpolation,
    complete_priority: bool,
    options: AnimationOptions,
}

/// One inert typed node in an atomic recursive live animation composition.
///
/// Nested children are owned so a caller can build a regular Rust tree without arenas or borrowed
/// slices. Only opaque semantic handles are borrowed; the tree does not contain a schedule,
/// runtime state, or a second scene representation.
#[derive(Clone)]
pub enum AnimationCompositionRequest<'a> {
    FocusOn {
        focus: crate::FocusOnOptions,
        options: AnimationOptions,
    },
    TransformTo(TransformToRequest<'a>),
    FamilyTransformTo {
        source: &'a MobjectFamily,
        target_state: &'a MobjectFamily,
        options: AnimationOptions,
    },
    Indicate {
        target: &'a Mobject,
        indication: IndicateOptions,
        options: AnimationOptions,
    },
    FamilyIndicate {
        target: &'a MobjectFamily,
        indication: IndicateOptions,
        options: AnimationOptions,
    },
    DrawBorderThenFill {
        target: &'a Mobject,
        outline: DrawBorderThenFillOptions,
        options: AnimationOptions,
    },
    FamilyDrawBorderThenFill {
        target: &'a MobjectFamily,
        outline: DrawBorderThenFillOptions,
        options: AnimationOptions,
    },
    FamilySubsetDisplay {
        target: &'a MobjectFamily,
        mode: SubsetDisplayMode,
        options: AnimationOptions,
    },
    /// Fade every ordered leaf while preserving the semantic family as the
    /// scene-membership unit.
    FamilyFade {
        target: &'a MobjectFamily,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    },
    /// Write or unwrite one plain Text object through its Rust-derived glyph members.
    TextWrite {
        target: &'a Mobject,
        reverse_member_order: bool,
        options: AnimationOptions,
    },
    /// Write or unwrite every ordered plain Text leaf as one global glyph sequence.
    FamilyTextWrite {
        target: &'a MobjectFamily,
        reverse_member_order: bool,
        options: AnimationOptions,
    },
    TextReveal {
        target: &'a Mobject,
        reverse: bool,
        options: AnimationOptions,
    },
    FamilyReveal {
        target: &'a MobjectFamily,
        reverse: bool,
        options: AnimationOptions,
    },
    PassingFlash {
        target: &'a Mobject,
        time_width: f64,
        options: AnimationOptions,
    },
    Rotate {
        target: &'a Mobject,
        angle: f64,
        options: AnimationOptions,
    },
    /// Exact centered 2D procedural rotation, with shared Manim pivot validation.
    ManimRotate {
        target: &'a Mobject,
        angle: f64,
        pivot: crate::ManimRotationPivot,
        options: AnimationOptions,
    },
    /// Animate one borrowed scalar tracker through the shared composition scheduler.
    ValueTracker {
        tracker: &'a ValueTracker,
        target: f64,
        options: AnimationOptions,
    },
    Wait {
        duration: f64,
    },
    Add {
        target: &'a Mobject,
        options: AnimationOptions,
    },
    Fade {
        target: &'a Mobject,
        direction: SemanticFadeDirection,
        endpoint: FadeEndpoint,
        options: AnimationOptions,
    },
    Create {
        target: &'a Mobject,
        options: AnimationOptions,
    },
    Uncreate {
        target: &'a Mobject,
        options: AnimationOptions,
    },
    AffineLifecycle {
        target: &'a Mobject,
        direction: AffineLifecycleDirection,
        endpoint: AffineLifecycleEndpoint,
        options: AnimationOptions,
    },
    Composition {
        kind: SemanticAnimationCompositionKind,
        children: Vec<AnimationCompositionRequest<'a>>,
        options: AnimationOptions,
    },
}

impl<'a> TransformToRequest<'a> {
    /// Complete an animated method target, including its exact painter priority.
    /// Ordinary Transform/MoveToTarget interpolation leaves priority unchanged.
    pub const fn method_target(mut self) -> Self {
        self.complete_priority = true;
        self
    }

    pub const fn new(
        source: &'a Mobject,
        target_state: &'a Mobject,
        options: AnimationOptions,
    ) -> Self {
        Self {
            source,
            target_state,
            interpolation: noon_core::SemanticTransformInterpolation::Affine,
            complete_priority: false,
            options,
        }
    }

    /// Request analytic point correspondence rather than affine-only interpolation.
    pub const fn point_correspondence(
        source: &'a Mobject,
        target_state: &'a Mobject,
        options: AnimationOptions,
    ) -> Self {
        Self {
            source,
            target_state,
            interpolation: noon_core::SemanticTransformInterpolation::PointCorrespondence,
            complete_priority: false,
            options,
        }
    }
}

/// Errors while a semantic handle is used through a live execution session.
#[derive(Debug)]
pub enum LiveSessionError {
    /// Shared authoring preflight failed before publication.
    Authoring(crate::AuthoringError),
    ForeignMobjectStore,
    // The remaining animation-specific shape checks are migrated in R2b.
    Mobject(String),
    Callback(crate::ExecutionSessionCallbackError),
    #[cfg(any(feature = "native-text", feature = "typst"))]
    Text(crate::TextAuthoringError),
    Animation(String),
    Activation(ExecutionSessionAnimationError),
    Segment(ExecutionSegmentError),
    Advance(ExecutionSegmentAdvanceError),
    Completion(ExecutionSegmentCompletionError),
    Publication(ExecutionSessionPublicationError),
}

impl std::fmt::Display for LiveSessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignMobjectStore => {
                formatter.write_str("mobject belongs to another semantic store")
            }
            Self::Authoring(error) => error.fmt(formatter),
            Self::Mobject(error) => error.fmt(formatter),
            Self::Callback(error) => error.fmt(formatter),
            #[cfg(any(feature = "native-text", feature = "typst"))]
            Self::Text(error) => error.fmt(formatter),
            Self::Animation(error) => error.fmt(formatter),
            Self::Activation(error) => error.fmt(formatter),
            Self::Segment(error) => error.fmt(formatter),
            Self::Advance(error) => error.fmt(formatter),
            Self::Completion(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LiveSessionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Authoring(error) => Some(error),
            Self::Callback(error) => Some(error),
            #[cfg(any(feature = "native-text", feature = "typst"))]
            Self::Text(error) => Some(error),
            Self::Activation(error) => Some(error),
            Self::Segment(error) => Some(error),
            Self::Advance(error) => Some(error),
            Self::Completion(error) => Some(error),
            Self::Publication(error) => Some(error),
            Self::ForeignMobjectStore | Self::Mobject(_) | Self::Animation(_) => None,
        }
    }
}

impl From<crate::AuthoringError> for LiveSessionError {
    fn from(error: crate::AuthoringError) -> Self {
        match error {
            // Retain the existing live error category for all foreign-handle paths.
            crate::AuthoringError::ForeignStore => Self::ForeignMobjectStore,
            other => Self::Authoring(other),
        }
    }
}

impl From<ExecutionSessionPublicationError> for LiveSessionError {
    fn from(value: ExecutionSessionPublicationError) -> Self {
        Self::Publication(value)
    }
}

impl From<ExecutionSessionAnimationError> for LiveSessionError {
    fn from(value: ExecutionSessionAnimationError) -> Self {
        Self::Activation(value)
    }
}

impl From<ExecutionSegmentError> for LiveSessionError {
    fn from(value: ExecutionSegmentError) -> Self {
        Self::Segment(value)
    }
}

impl From<ExecutionSegmentAdvanceError> for LiveSessionError {
    fn from(value: ExecutionSegmentAdvanceError) -> Self {
        Self::Advance(value)
    }
}

impl From<ExecutionSegmentCompletionError> for LiveSessionError {
    fn from(value: ExecutionSegmentCompletionError) -> Self {
        Self::Completion(value)
    }
}

/// A temporary, typed view over one semantic store and its published runtime.
///
/// `LiveSession` has no scheduler, scene copy, or runtime mirror.  Persistent
/// property edits use the shared semantic transaction vocabulary and publish
/// through [`ExecutionSession`] atomically.  The supported transaction subset is
/// exactly the session publication subset.
pub struct LiveSession<'a> {
    store: &'a Rc<RefCell<SemanticStore>>,
    root: SemanticNodeId,
    session: &'a mut ExecutionSession,
}

impl<'a> LiveSession<'a> {
    /// Create and sparsely enroll one scalar tracker in this already-live Scene.
    pub fn value_tracker(&mut self, initial: f64) -> Result<ValueTracker, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        let node = self
            .session
            .create_scoped_value_tracker(&mut store, self.root, initial)?;
        Ok(ValueTracker::from_semantic_node(
            Rc::clone(self.store),
            node,
        ))
    }

    /// Associate one existing detached tracker with this live Scene root.
    pub fn associate_value_tracker(
        &mut self,
        tracker: &ValueTracker,
    ) -> Result<(), LiveSessionError> {
        tracker
            .require_store(self.store)
            .map_err(LiveSessionError::from)?;
        let mut store = self.store.borrow_mut();
        self.session
            .associate_value_tracker(&mut store, self.root, tracker.node_id())
            .map_err(Into::into)
    }

    /// Bind a facade to the supplied store and existing execution session.
    /// Provenance and revision are checked by every publish/query operation.
    pub fn new(
        store: &'a Rc<RefCell<SemanticStore>>,
        root: SemanticNodeId,
        session: &'a mut ExecutionSession,
    ) -> Self {
        Self {
            store,
            root,
            session,
        }
    }

    /// Apply one supported semantic transaction and publish it into the same
    /// runtime. Unsupported content and structural work fails before
    /// either layer commits.
    pub fn apply(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .apply_semantic_transaction_at_root(&mut store, self.root, transaction)
            .map_err(Into::into)
    }

    /// Add an existing detached object to this live scene root.
    pub fn add(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Add(&[
            MobjectFamilyMember::Mobject(mobject),
        ]))
    }

    /// Remove an existing object from this live scene root without deleting identity.
    pub fn remove(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Remove(&[
            MobjectFamilyMember::Mobject(mobject),
        ]))
    }

    pub fn edit_membership(
        &mut self,
        request: SceneMembershipRequest<'_>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let transaction =
            crate::scene_membership::prepare_scene_membership(self.store, self.root, request)?;
        self.apply(transaction)
    }

    pub fn add_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Add(members))
    }

    pub fn remove_many(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Remove(members))
    }

    pub fn clear(&mut self) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Clear)
    }

    pub fn replace(
        &mut self,
        old: MobjectFamilyMember<'_>,
        new: MobjectFamilyMember<'_>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_membership(SceneMembershipRequest::Replace { old, new })
    }

    /// Check whether a handle is currently a direct member of this live scene root.
    pub fn contains(&self, mobject: &Mobject) -> Result<bool, LiveSessionError> {
        self.require_mobject(mobject)?;
        self.store
            .borrow()
            .is_direct_member(self.root, mobject.node_id())
            .map_err(|error| {
                crate::AuthoringError::from(noon_core::SemanticSceneOperationError::from(error))
                    .into()
            })
    }

    /// Replace one live object's content with content already authored in this store.
    /// Transform, style, semantic identity, and family membership stay unchanged.
    pub fn replace_content(
        &mut self,
        target: &Mobject,
        source: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(target)?;
        self.require_mobject(source)?;
        let content = source.state().map_err(LiveSessionError::from)?.content;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(target.node_id(), content);
        self.apply(transaction)
    }

    /// Inspect authored/base state explicitly, separate from [`Self::effective`].
    pub fn authored(&self, mobject: &Mobject) -> Result<SemanticObjectState, LiveSessionError> {
        self.require_mobject(mobject)?;
        mobject.state().map_err(LiveSessionError::from)
    }

    /// Create a detached, session-coherent target copy for subsequent live authoring.
    ///
    /// Detached target edits advance the same semantic/runtime publication context but produce
    /// no execution object or frame work. This keeps later atomic animation declaration valid
    /// without resetting or relowering the active runtime.
    pub fn target_editor(&mut self, source: &Mobject) -> Result<Mobject, LiveSessionError> {
        self.require_mobject(source)?;
        self.require_target_capture()?;

        let state = self.capture_mobject_state(source)?;

        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(noon_core::SemanticNodeCreation::object(state));
        let result = self.apply(transaction)?;
        let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("one prepared target copy has one exact semantic impact")
        };
        Mobject::from_node(Rc::clone(self.store), *node).map_err(LiveSessionError::from)
    }

    fn require_target_capture(&self) -> Result<(), LiveSessionError> {
        self.session.require_published_store(&self.store.borrow())?;
        if let Some(token) = self.session.pending_callback_token() {
            return Err(LiveSessionError::Callback(
                crate::ExecutionSessionCallbackError::Pending(token),
            ));
        }
        if let Some(termination) = self.session.callback_termination() {
            return Err(LiveSessionError::Callback(
                crate::ExecutionSessionCallbackError::Terminated(termination),
            ));
        }

        Ok(())
    }

    /// Copy a complete family from this coherent runtime in one publication.
    pub fn copy_family(
        &mut self,
        source: &MobjectFamily,
    ) -> Result<crate::FamilyCopy, LiveSessionError> {
        self.copy_family_with_references(source, &[])
    }

    /// Copy a family and detached metadata references from one coherent state.
    pub fn copy_family_with_references(
        &mut self,
        source: &MobjectFamily,
        references: &[crate::MobjectFamilyMember<'_>],
    ) -> Result<crate::FamilyCopy, LiveSessionError> {
        self.require_family(source)?;
        self.require_target_capture()?;
        let (transaction, pending) =
            crate::family_copy::prepare_family_copy(source, references, |mobject| {
                self.capture_mobject_state(mobject)
            })?;
        let result = self.apply(transaction)?;
        pending.resolve(&result).map_err(LiveSessionError::from)
    }

    /// Replace one object's presentation with another object's effective state while
    /// retaining the target identity and scene membership.
    pub fn become_mobject(
        &mut self,
        target: &Mobject,
        other: &Mobject,
        options: ManimBecomeOptions,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(target)?;
        self.require_mobject(other)?;
        let authored = target.state().map_err(LiveSessionError::from)?;
        let source = self.capture_mobject_state(target)?;
        let candidate = self.capture_mobject_state(other)?;
        let next = prepare_become_state(&self.store.borrow(), &source, candidate, options)
            .map_err(LiveSessionError::from)?;
        let mut transaction = SemanticMutationTransaction::new();
        stage_state_changes(&mut transaction, target.node_id(), &authored, &next);
        self.apply(transaction)
    }

    /// Replace a matching family's presentation from one coherent capture.
    /// Existing member identities/order survive, and all edits publish together.
    pub fn become_family(
        &mut self,
        source: &MobjectFamily,
        target: &MobjectFamily,
        options: ManimBecomeOptions,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(source)?;
        self.require_family(target)?;
        self.require_target_capture()?;
        let transaction = crate::state_replacement::family_become_transaction(
            source,
            target,
            options,
            |object| self.capture_mobject_state(object),
        )?;
        self.apply(transaction)
    }

    fn capture_mobject_state(
        &self,
        source: &Mobject,
    ) -> Result<SemanticObjectState, LiveSessionError> {
        // A reachable object starts from the coherent effective row rather than
        // an authored base superseded by a driver. A detached object has no row,
        // so its authored state is the exact capture. Immutable content remains
        // authored because effective render-content overrides are rejected.
        let mut state = source.state().map_err(LiveSessionError::from)?;
        if !state.signal_bindings().is_empty() {
            return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::CaptureReactiveBinding,
            )));
        }
        if self.session.semantic_object_is_reachable(source.node_id()) {
            let store = self.store.borrow();
            let observed = self
                .session
                .effective_semantic_object(&store, source.node_id())?;
            if !observed.authored_content_layout_applicable() {
                return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::CaptureRenderOverride,
                )));
            }
            if observed.object.appearance != 1.0 {
                return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::CaptureNonUnitAppearance,
                )));
            }
            preserve_or_capture_f32(
                &mut state.transform.translation.x,
                observed.object.transform.translation.x,
            );
            preserve_or_capture_f32(
                &mut state.transform.translation.y,
                observed.object.transform.translation.y,
            );
            preserve_or_capture_f32(
                &mut state.transform.scale.x,
                observed.object.transform.scale.x,
            );
            preserve_or_capture_f32(
                &mut state.transform.scale.y,
                observed.object.transform.scale.y,
            );
            preserve_or_capture_f32(
                &mut state.transform.rotation_z,
                observed.object.transform.rotation,
            );
            state.set_z_index(observed.object.z_index);
            state.style = target_style_from_effective(&state.style, observed.object.style)?;
        }
        Ok(state)
    }

    /// Publish one detached ordered family through this session's semantic owner.
    pub fn family(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<MobjectFamily, LiveSessionError> {
        self.family_with_z_index(members, 0.0)
    }

    /// Atomically create a detached family with root-only painter priority.
    pub fn family_with_z_index(
        &mut self,
        members: &[MobjectFamilyMember<'_>],
        z_index: f64,
    ) -> Result<MobjectFamily, LiveSessionError> {
        let (transaction, family) =
            crate::family_authoring::family_creation_transaction(self.store, members, z_index)
                .map_err(LiveSessionError::from)?;
        let result = self.apply(transaction)?;
        let node = result
            .resolve(family)
            .expect("committed family token resolves to one semantic identity");
        MobjectFamily::from_node(Rc::clone(self.store), node).map_err(LiveSessionError::from)
    }

    /// Publish one atomic batch of direct family additions.
    pub fn add_family_members(
        &mut self,
        family: &MobjectFamily,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Vec<bool>, LiveSessionError> {
        self.edit_family_members(family, members, true)
    }

    pub fn remove_family_members(
        &mut self,
        family: &MobjectFamily,
        members: &[MobjectFamilyMember<'_>],
    ) -> Result<Vec<bool>, LiveSessionError> {
        self.edit_family_members(family, members, false)
    }

    fn edit_family_members(
        &mut self,
        family: &MobjectFamily,
        members: &[MobjectFamilyMember<'_>],
        adding: bool,
    ) -> Result<Vec<bool>, LiveSessionError> {
        self.require_family(family)?;
        let (transaction, changed) =
            crate::family_authoring::family_membership_transaction(family, members, adding)
                .map_err(LiveSessionError::from)?;
        self.apply(transaction)?;
        Ok(changed)
    }

    /// Publish one fully validated detached Manim geometry object through this session.
    ///
    /// The new identity has no root membership, execution slot, or frame work
    /// until [`Self::add`] admits it.
    pub fn create_manim_geometry(
        &mut self,
        options: crate::ManimGeometryOptions,
    ) -> Result<Mobject, LiveSessionError> {
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let state = options
            .into_state(&mut self.store.borrow_mut())
            .map_err(LiveSessionError::from)?;
        self.create_detached_mobject(state)
    }

    /// Shape and publish one detached plain Text object through this live session.
    ///
    /// The object receives semantic identity and immutable text resources, but no
    /// scene membership or execution row until a later Add, FadeIn, or Create.
    #[cfg(feature = "native-text")]
    pub fn create_text(&mut self, text: crate::Text) -> Result<Mobject, LiveSessionError> {
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let state = crate::text_authoring::native_text_state(self.store, text)
            .map_err(LiveSessionError::Text)?;
        self.create_detached_mobject(state)
    }

    /// Compile and publish one detached Typst object through this live session.
    #[cfg(feature = "typst")]
    pub fn create_typst(&mut self, text: crate::Typst) -> Result<Mobject, LiveSessionError> {
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let state =
            crate::text_authoring::typst_state(self.store, text).map_err(LiveSessionError::Text)?;
        self.create_detached_mobject(state)
    }

    /// Compile and publish one detached MathTypst object through this live session.
    #[cfg(feature = "typst")]
    pub fn create_math_typst(
        &mut self,
        text: crate::MathTypst,
    ) -> Result<Mobject, LiveSessionError> {
        self.session
            .require_resource_creation_at_root(&self.store.borrow(), self.root)?;
        let state = crate::text_authoring::math_typst_state(self.store, text)
            .map_err(LiveSessionError::Text)?;
        self.create_detached_mobject(state)
    }

    fn create_detached_mobject(
        &mut self,
        state: SemanticObjectState,
    ) -> Result<Mobject, LiveSessionError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(noon_core::SemanticNodeCreation::object(state));
        let result = self.apply(transaction)?;
        let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("one detached primitive creation has one exact semantic impact")
        };
        Mobject::from_node(Rc::clone(self.store), *node).map_err(LiveSessionError::from)
    }

    /// Read the current effective runtime value at the session's publication.
    pub fn effective(&self, mobject: &Mobject) -> Result<EffectiveMobjectState, LiveSessionError> {
        self.require_mobject(mobject)?;
        let store = self.store.borrow();
        let EffectiveSemanticObject {
            object,
            publication,
            ..
        } = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        Ok(EffectiveMobjectState {
            z_index: object.z_index,
            transform: object.transform,
            style: object.style,
            appearance: object.appearance,
            publication,
        })
    }

    /// Read exact layout values from authored content at the current effective
    /// transform. Work and resource lookup are bounded to this object.
    pub fn effective_layout(
        &self,
        mobject: &Mobject,
    ) -> Result<EffectiveMobjectLayout, LiveSessionError> {
        self.require_mobject(mobject)?;
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::EffectiveLayoutRenderOverride,
            )));
        }
        let transform = observed.object.transform;
        let publication = observed.publication;
        drop(store);
        self.layout_at_transform(mobject, transform, publication)
    }

    fn validate_manim_rotation_pivot(
        &self,
        target: &Mobject,
        pivot: crate::ManimRotationPivot,
    ) -> Result<(), LiveSessionError> {
        let (bounds, origin) = if self.session.semantic_object_is_reachable(target.node_id()) {
            let store = self.store.borrow();
            let observed = self
                .session
                .effective_semantic_object(&store, target.node_id())?;
            if !observed.authored_content_layout_applicable() {
                return Err(LiveSessionError::Mobject("procedural rotation requires effective authored content without reveal or morph overrides".into()));
            }
            let transform = observed.object.transform;
            drop(store);
            (
                target.boundary_bounds_at(transform),
                (
                    f64::from(transform.translation.x),
                    f64::from(transform.translation.y),
                ),
            )
        } else {
            let state = target.state().map_err(LiveSessionError::from)?;
            (
                target.boundary_bounds(),
                (state.transform.translation.x, state.transform.translation.y),
            )
        };
        pivot
            .validate(bounds.map_err(LiveSessionError::from)?, origin)
            .map_err(LiveSessionError::Mobject)
    }

    /// Read one analytic Line's world endpoints at the current publication.
    pub fn effective_line_endpoints(
        &self,
        mobject: &Mobject,
    ) -> Result<ManimLineEndpoints, LiveSessionError> {
        self.require_mobject(mobject)?;
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, mobject.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::EffectiveLineRenderOverride,
            )));
        }
        let transform = observed.object.transform;
        drop(store);
        mobject
            .manim_line_endpoints_at(transform)
            .map_err(LiveSessionError::from)
    }

    pub fn effective_fill_color(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Color>, LiveSessionError> {
        mobject.fill_color().map_err(LiveSessionError::from)?;
        Ok(self
            .effective(mobject)?
            .style
            .fill
            .map(crate::semantic_mobject::opaque_paint_color))
    }

    pub fn effective_stroke_color(
        &self,
        mobject: &Mobject,
    ) -> Result<Option<Color>, LiveSessionError> {
        mobject.stroke_color().map_err(LiveSessionError::from)?;
        Ok(self
            .effective(mobject)?
            .style
            .stroke
            .map(crate::semantic_mobject::opaque_paint_color))
    }

    pub fn effective_stroke_width(&self, mobject: &Mobject) -> Result<f64, LiveSessionError> {
        Ok(f64::from(self.effective(mobject)?.style.stroke_width))
    }

    /// Read visible fill RGB, falling back to stroke RGB, at the current publication.
    pub fn effective_manim_color(&self, mobject: &Mobject) -> Result<Color, LiveSessionError> {
        // Resource paints do not have a scalar Manim color representation. Check
        // the selected authored channel before observing its lowered runtime style.
        mobject.manim_color().map_err(LiveSessionError::from)?;
        let effective = self.effective(mobject)?;
        Ok(crate::semantic_mobject::manim_color_from_effective(
            &effective.style,
        ))
    }

    fn layout_at_transform(
        &self,
        mobject: &Mobject,
        transform: Transform2D,
        publication: PublicationContext,
    ) -> Result<EffectiveMobjectLayout, LiveSessionError> {
        let bounds = mobject.layout_bounds_at(transform)?;
        let boundary = mobject.boundary_bounds_at(transform)?;
        let center = boundary.map_or(
            (
                f64::from(transform.translation.x),
                f64::from(transform.translation.y),
            ),
            |b| ((b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5),
        );
        Ok(EffectiveMobjectLayout {
            center,
            width: bounds.map_or(0.0, Bounds2D64::width),
            height: bounds.map_or(0.0, Bounds2D64::height),
            publication,
        })
    }

    /// Activate one predeclared animation in this session.
    ///
    /// This performs no semantic declaration or target creation: the supplied
    /// handle is replayable authored state, while activation atomically adds
    /// execution-local tracks and captures the current effective affine source.
    /// The returned segment can be driven with [`Self::advance_segment_to`] and
    /// observed with [`Self::segment_state`]. Call [`Self::complete_segment`]
    /// at its coherent endpoint before sequential authoring resumes.
    pub fn play_animation(
        &mut self,
        animation: &DeclaredAnimation,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        animation
            .require_store(self.store)
            .map_err(LiveSessionError::Animation)?;
        let store = self.store.borrow();
        let options = store
            .semantic_animation_state(animation.node_id())
            .map_err(|error| LiveSessionError::Animation(error.to_string()))?
            .options();
        self.session
            .activate_animation_segment(&store, animation.node_id(), options)
            .map_err(Into::into)
    }

    /// Atomically author and activate one supported transform/style transition after bootstrap.
    ///
    /// The declaration and execution tracks publish together through the canonical semantic
    /// transaction and runtime. The returned segment uses the existing advance/completion
    /// lifecycle and this facade retains no animation target or scheduler state.
    pub fn declare_and_activate_transform_to(
        &mut self,
        source: &Mobject,
        target: &Mobject,
        options: noon_core::AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.require_mobject(source)?;
        self.require_mobject(target)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_transform_to(
                &mut store,
                source.node_id(),
                target.node_id(),
                options,
            )
            .map_err(Into::into)
    }

    /// Transform two equivalent semantic families through one ordered composition.
    pub fn declare_and_activate_family_transform_to(
        &mut self,
        source: &MobjectFamily,
        target_state: &MobjectFamily,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilyTransformTo {
            source,
            target_state,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Indicate one object and restore its activation-effective source state.
    pub fn declare_and_activate_indicate(
        &mut self,
        target: &Mobject,
        indication: IndicateOptions,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::Indicate {
            target,
            indication,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Indicate an ordered semantic family and restore every effective source state.
    pub fn declare_and_activate_family_indicate(
        &mut self,
        target: &MobjectFamily,
        indication: IndicateOptions,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilyIndicate {
            target,
            indication,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Reveal one vector outline and restore its activation-effective final style.
    pub fn declare_and_activate_draw_border_then_fill(
        &mut self,
        target: &Mobject,
        outline: DrawBorderThenFillOptions,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::DrawBorderThenFill {
            target,
            outline,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Draw an ordered vector family through one atomic lagged composition.
    pub fn declare_and_activate_family_draw_border_then_fill(
        &mut self,
        target: &MobjectFamily,
        outline: DrawBorderThenFillOptions,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilyDrawBorderThenFill {
            target,
            outline,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Display an ordered family through the shared floor/ceil subset thresholds.
    pub fn declare_and_activate_family_subset_display(
        &mut self,
        target: &MobjectFamily,
        mode: SubsetDisplayMode,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilySubsetDisplay {
            target,
            mode,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Fade an ordered semantic family through one atomic membership lifecycle.
    pub fn declare_and_activate_family_fade(
        &mut self,
        target: &MobjectFamily,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilyFade {
            target,
            direction,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Write or unwrite one plain Text object through shared glyph semantics.
    pub fn declare_and_activate_text_write(
        &mut self,
        target: &Mobject,
        reverse_member_order: bool,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::TextWrite {
            target,
            reverse_member_order,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Write or unwrite an ordered plain-Text family through one global glyph plan.
    pub fn declare_and_activate_family_text_write(
        &mut self,
        target: &MobjectFamily,
        reverse_member_order: bool,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::FamilyTextWrite {
            target,
            reverse_member_order,
            options,
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    pub fn declare_and_activate_text_reveal(
        &mut self,
        target: &Mobject,
        reverse: bool,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.declare_and_activate_composition(
            &AnimationCompositionRequest::TextReveal {
                target,
                reverse,
                options,
            },
            AnimationOptions::new(),
        )
    }

    pub fn declare_and_activate_family_reveal(
        &mut self,
        target: &MobjectFamily,
        reverse: bool,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.declare_and_activate_composition(
            &AnimationCompositionRequest::FamilyReveal {
                target,
                reverse,
                options,
            },
            AnimationOptions::new(),
        )
    }

    /// Construct, animate and remove a fixed-point spotlight in one shared session.
    pub fn declare_and_activate_focus_on(
        &mut self,
        focus: crate::FocusOnOptions,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.declare_and_activate_composition(
            &AnimationCompositionRequest::FocusOn { focus, options },
            AnimationOptions::new(),
        )
    }

    /// Flash one exact analytic Line through fixed transient membership.
    pub fn declare_and_activate_passing_flash(
        &mut self,
        target: &Mobject,
        time_width: f64,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.declare_and_activate_composition(
            &AnimationCompositionRequest::PassingFlash {
                target,
                time_width,
                options,
            },
            AnimationOptions::new(),
        )
    }

    /// Atomically hide every direct member before activating a subset display.
    pub fn prepare_family_subset_display(
        &mut self,
        target: &MobjectFamily,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(target)?;
        let transaction = {
            let store = self.store.borrow();
            crate::family_authoring::prepare_subset_display_transaction(&store, target.node_id())
                .map_err(LiveSessionError::Mobject)?
        };
        self.apply(transaction)
    }

    /// Atomically append and activate one scalar tracker interval at the current
    /// session time. The returned segment uses the same completion barrier as
    /// object-property animation tracks.
    pub fn declare_and_activate_value_tracker(
        &mut self,
        tracker: &ValueTracker,
        target: f64,
        duration: f64,
        rate_func: noon_core::RateFunction,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::ValueTracker {
            tracker,
            target,
            options: AnimationOptions::new()
                .run_time(duration)
                .rate_func(rate_func),
        };
        self.declare_and_activate_composition(&request, AnimationOptions::new())
    }

    /// Persist one tracker value at the current live authored time after its
    /// active segment has completed and released timeline ownership.
    pub fn set_value(
        &mut self,
        tracker: &ValueTracker,
        value: f64,
    ) -> Result<(), LiveSessionError> {
        tracker
            .require_store(self.store)
            .map_err(LiveSessionError::from)?;
        let mut store = self.store.borrow_mut();
        self.session
            .set_scalar_signal_value(&mut store, tracker.node_id(), value)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Atomically author and activate one canonical single-leaf FadeIn or FadeOut.
    pub fn declare_and_activate_fade(
        &mut self,
        target: &Mobject,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.declare_and_activate_fade_with_endpoint(
            target,
            direction,
            FadeEndpoint::default(),
            options,
        )
    }

    /// Atomically author and activate a Fade with a scaled/translated faded endpoint.
    pub fn declare_and_activate_fade_with_endpoint(
        &mut self,
        target: &Mobject,
        direction: SemanticFadeDirection,
        endpoint: FadeEndpoint,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.require_mobject(target)?;
        let endpoint = self.resolve_fade_endpoint(target, endpoint)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_fade_with_endpoint(
                &mut store,
                self.root,
                target.node_id(),
                direction,
                endpoint,
                options,
            )
            .map_err(Into::into)
    }

    /// Atomically introduce one detached leaf and activate shared Create reveal semantics.
    pub fn declare_and_activate_create(
        &mut self,
        target: &Mobject,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.require_mobject(target)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_create(&mut store, self.root, target.node_id(), options)
            .map_err(Into::into)
    }

    /// Atomically author and activate one Grow/Spin/Shrink affine lifecycle.
    pub fn declare_and_activate_affine_lifecycle(
        &mut self,
        target: &Mobject,
        direction: AffineLifecycleDirection,
        endpoint: AffineLifecycleEndpoint,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.require_mobject(target)?;
        let endpoint = self.resolve_affine_lifecycle_endpoint(target, direction, endpoint)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_affine_lifecycle(
                &mut store,
                self.root,
                target.node_id(),
                direction,
                endpoint,
                options,
            )
            .map_err(Into::into)
    }

    /// Reverse one leaf's Reveal and remove it at completion.
    ///
    /// The shared declaration accepts either a detached leaf, which it admits atomically, or
    /// one direct live member of this session root. In both cases completion owns removal.
    pub fn declare_and_activate_uncreate(
        &mut self,
        target: &Mobject,
        options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        self.require_mobject(target)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_uncreate(&mut store, self.root, target.node_id(), options)
            .map_err(Into::into)
    }

    /// Atomically introduce detached leaves through one flat Parallel Create segment.
    ///
    /// The shared execution session validates every handle before staging membership,
    /// declarations, reveal tracks, and runtime publication in one transaction.
    pub fn declare_and_activate_create_parallel(
        &mut self,
        children: &[(&Mobject, AnimationOptions)],
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        for (target, _) in children {
            self.require_mobject(target)?;
        }
        let children = children
            .iter()
            .map(|(target, options)| (target.node_id(), *options))
            .collect::<Vec<_>>();
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_create_parallel(&mut store, self.root, &children, play_options)
            .map_err(Into::into)
    }

    /// Atomically author and activate one recursive shared composition.
    ///
    /// Every opaque handle is checked before the session receives the inert tree. The session then
    /// validates membership/lifecycle constraints and stages one semantic mutation transaction;
    /// this facade owns neither a schedule nor a runtime copy.
    pub fn declare_and_activate_composition(
        &mut self,
        request: &AnimationCompositionRequest<'_>,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = self.execution_composition_request(request)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_composition(&mut store, self.root, &request, play_options)
            .map_err(Into::into)
    }

    /// Direct transform convenience built on the same recursive declaration path.
    pub fn declare_and_activate_transform_composition(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: &[TransformToRequest<'_>],
        composition_options: AnimationOptions,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::Composition {
            kind,
            children: children
                .iter()
                .copied()
                .map(AnimationCompositionRequest::TransformTo)
                .collect(),
            options: composition_options,
        };
        self.declare_and_activate_composition(&request, play_options)
    }

    /// Flat caller convenience over the same recursive composition operation.
    pub fn declare_and_activate_animation_composition(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: &[AnimationCompositionRequest<'_>],
        composition_options: AnimationOptions,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        let request = AnimationCompositionRequest::Composition {
            kind,
            children: children.to_vec(),
            options: composition_options,
        };
        self.declare_and_activate_composition(&request, play_options)
    }

    fn resolve_affine_lifecycle_endpoint(
        &self,
        target: &Mobject,
        direction: AffineLifecycleDirection,
        endpoint: AffineLifecycleEndpoint,
    ) -> Result<SemanticAffineLifecycleEndpoint, LiveSessionError> {
        match endpoint {
            AffineLifecycleEndpoint::Point {
                x,
                y,
                rotation_offset,
                point_color,
            } => Ok(SemanticAffineLifecycleEndpoint {
                point: SemanticVec3::new(x, y, 0.0),
                rotation_offset,
                point_color,
            }),
            AffineLifecycleEndpoint::EffectiveCenter => {
                if direction != AffineLifecycleDirection::RemoveTo {
                    return Err(LiveSessionError::Mobject(
                        "effective-center lifecycle endpoints require a live removal target".into(),
                    ));
                }
                let center = if self.contains(target)? {
                    self.effective_layout(target)?.center
                } else {
                    target.center().map_err(LiveSessionError::from)?
                };
                Ok(SemanticAffineLifecycleEndpoint {
                    point: SemanticVec3::new(center.0, center.1, 0.0),
                    rotation_offset: 0.0,
                    point_color: None,
                })
            }
        }
    }

    fn execution_composition_request(
        &self,
        request: &AnimationCompositionRequest<'_>,
    ) -> Result<crate::execution_session::SemanticCompositionRequest, LiveSessionError> {
        use crate::execution_session::SemanticCompositionRequest as Request;
        Ok(match request {
            AnimationCompositionRequest::FocusOn { focus, options } => Request::FocusOn {
                focus: *focus,
                options: *options,
            },
            AnimationCompositionRequest::TransformTo(child) => {
                self.require_mobject(child.source)?;
                self.require_mobject(child.target_state)?;
                Request::TransformTo {
                    source: child.source.node_id(),
                    target_state: child.target_state.node_id(),
                    interpolation: child.interpolation,
                    complete_priority: child.complete_priority,
                    options: child.options,
                }
            }
            AnimationCompositionRequest::FamilyTransformTo {
                source,
                target_state,
                options,
            } => {
                self.require_family(source)?;
                self.require_family(target_state)?;
                Request::FamilyTransformTo {
                    source: source.node_id(),
                    target_state: target_state.node_id(),
                    options: *options,
                }
            }
            AnimationCompositionRequest::Indicate {
                target,
                indication,
                options,
            } => {
                self.require_mobject(target)?;
                Request::Indicate {
                    target: target.node_id(),
                    indication: *indication,
                    scale_center: {
                        let center = self.effective_layout(target)?.center;
                        SemanticVec3::new(center.0, center.1, 0.0)
                    },
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilyIndicate {
                target,
                indication,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilyIndicate {
                    target: target.node_id(),
                    indication: *indication,
                    scale_center: self.family_effective_center(target)?,
                    options: *options,
                }
            }
            AnimationCompositionRequest::DrawBorderThenFill {
                target,
                outline,
                options,
            } => {
                self.require_mobject(target)?;
                Request::DrawBorderThenFill {
                    target: target.node_id(),
                    outline: *outline,
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilyDrawBorderThenFill {
                target,
                outline,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilyDrawBorderThenFill {
                    target: target.node_id(),
                    outline: *outline,
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilySubsetDisplay {
                target,
                mode,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilySubsetDisplay {
                    target: target.node_id(),
                    mode: *mode,
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilyFade {
                target,
                direction,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilyFade {
                    target: target.node_id(),
                    direction: *direction,
                    options: *options,
                }
            }
            AnimationCompositionRequest::TextWrite {
                target,
                reverse_member_order,
                options,
            } => {
                self.require_mobject(target)?;
                Request::TextWrite {
                    target: target.node_id(),
                    reverse_member_order: *reverse_member_order,
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilyTextWrite {
                target,
                reverse_member_order,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilyTextWrite {
                    target: target.node_id(),
                    reverse_member_order: *reverse_member_order,
                    options: *options,
                }
            }
            AnimationCompositionRequest::TextReveal {
                target,
                reverse,
                options,
            } => {
                self.require_mobject(target)?;
                Request::TextReveal {
                    target: target.node_id(),
                    reverse: *reverse,
                    options: *options,
                }
            }
            AnimationCompositionRequest::FamilyReveal {
                target,
                reverse,
                options,
            } => {
                self.require_family(target)?;
                Request::FamilyReveal {
                    target: target.node_id(),
                    reverse: *reverse,
                    options: *options,
                }
            }
            AnimationCompositionRequest::PassingFlash {
                target,
                time_width,
                options,
            } => {
                self.require_mobject(target)?;
                Request::PassingFlash {
                    target: target.node_id(),
                    time_width: *time_width,
                    options: *options,
                }
            }
            AnimationCompositionRequest::Rotate {
                target,
                angle,
                options,
            } => {
                self.require_mobject(target)?;
                Request::Rotate {
                    target: target.node_id(),
                    angle: *angle,
                    hold_origin: false,
                    options: *options,
                }
            }
            AnimationCompositionRequest::ManimRotate {
                target,
                angle,
                pivot,
                options,
            } => {
                self.require_mobject(target)?;
                self.validate_manim_rotation_pivot(target, *pivot)?;
                Request::Rotate {
                    target: target.node_id(),
                    angle: *angle,
                    hold_origin: true,
                    options: *options,
                }
            }
            AnimationCompositionRequest::ValueTracker {
                tracker,
                target,
                options,
            } => {
                tracker
                    .require_store(self.store)
                    .map_err(LiveSessionError::from)?;
                Request::ValueTracker {
                    signal: tracker.node_id(),
                    target: *target,
                    options: *options,
                }
            }
            AnimationCompositionRequest::Wait { duration } => Request::Wait {
                duration: *duration,
            },
            AnimationCompositionRequest::Add { target, options } => {
                self.require_mobject(target)?;
                Request::Add {
                    target: target.node_id(),
                    options: *options,
                }
            }
            AnimationCompositionRequest::Fade {
                target,
                direction,
                endpoint,
                options,
            } => {
                self.require_mobject(target)?;
                Request::Fade {
                    target: target.node_id(),
                    direction: *direction,
                    endpoint: self.resolve_fade_endpoint(target, *endpoint)?,
                    options: *options,
                }
            }
            AnimationCompositionRequest::Create { target, options } => {
                self.require_mobject(target)?;
                Request::Create {
                    target: target.node_id(),
                    options: *options,
                }
            }
            AnimationCompositionRequest::Uncreate { target, options } => {
                self.require_mobject(target)?;
                Request::Uncreate {
                    target: target.node_id(),
                    options: *options,
                }
            }
            AnimationCompositionRequest::AffineLifecycle {
                target,
                direction,
                endpoint,
                options,
            } => {
                self.require_mobject(target)?;
                Request::AffineLifecycle {
                    target: target.node_id(),
                    direction: *direction,
                    endpoint: self
                        .resolve_affine_lifecycle_endpoint(target, *direction, *endpoint)?,
                    options: *options,
                }
            }
            AnimationCompositionRequest::Composition {
                kind,
                children,
                options,
            } => Request::Composition {
                kind: *kind,
                children: children
                    .iter()
                    .map(|child| self.execution_composition_request(child))
                    .collect::<Result<Vec<_>, _>>()?,
                options: *options,
            },
        })
    }

    fn resolve_fade_endpoint(
        &self,
        target: &Mobject,
        endpoint: FadeEndpoint,
    ) -> Result<noon_core::SemanticFadeEndpoint, LiveSessionError> {
        let needs_layout = endpoint.scale_factor != 1.0
            || matches!(endpoint.translation, FadeTranslation::Point(_));
        let center = if needs_layout {
            Some(if self.contains(target)? {
                self.effective_layout(target)?.center
            } else {
                target.center().map_err(LiveSessionError::from)?
            })
        } else {
            None
        };
        let scale_center = center.map_or(SemanticVec3::ZERO, |center| {
            SemanticVec3::new(center.0, center.1, 0.0)
        });
        let translation = match endpoint.translation {
            FadeTranslation::Shift(shift) => noon_core::SemanticFadeTranslation::Shift(shift),
            FadeTranslation::Point(point) => {
                let center = center.expect("point Fade endpoint resolves effective layout");
                noon_core::SemanticFadeTranslation::PointOffset(SemanticVec3::new(
                    point.x - center.0,
                    point.y - center.1,
                    point.z,
                ))
            }
        };
        Ok(noon_core::SemanticFadeEndpoint {
            scale_factor: endpoint.scale_factor,
            translation,
            scale_center,
        })
    }

    fn family_effective_center(
        &self,
        family: &MobjectFamily,
    ) -> Result<SemanticVec3, LiveSessionError> {
        let leaves = self
            .store
            .borrow()
            .ordered_family_leaf_pairs(family.node_id(), family.node_id())
            .map_err(crate::AuthoringError::from)?
            .into_iter()
            .map(|(leaf, _)| leaf)
            .collect::<Vec<_>>();
        let mut bounds: Option<Bounds2D64> = None;
        for leaf in leaves {
            let leaf =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            let layout = self.effective_layout(&leaf)?;
            let leaf_bounds = Bounds2D64 {
                min_x: layout.center.0 - layout.width * 0.5,
                min_y: layout.center.1 - layout.height * 0.5,
                max_x: layout.center.0 + layout.width * 0.5,
                max_y: layout.center.1 + layout.height * 0.5,
            };
            bounds = Some(match bounds {
                Some(current) => Bounds2D64 {
                    min_x: current.min_x.min(leaf_bounds.min_x),
                    min_y: current.min_y.min(leaf_bounds.min_y),
                    max_x: current.max_x.max(leaf_bounds.max_x),
                    max_y: current.max_y.max(leaf_bounds.max_y),
                },
                None => leaf_bounds,
            });
        }
        let bounds = bounds.expect("validated semantic family has at least one ordered leaf");
        Ok(SemanticVec3::new(
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
            0.0,
        ))
    }

    /// Start a continuation wait without allocating a scheduler track.
    pub fn wait_segment(&self, duration: f64) -> Result<ExecutionSegment, LiveSessionError> {
        self.session.wait_segment(duration).map_err(Into::into)
    }

    /// Observe a logical continuation segment against the shared runtime.
    pub fn segment_state(&self, segment: ExecutionSegment) -> ExecutionSegmentState {
        self.session.segment_state(segment)
    }

    /// Drive one segment toward its exact endpoint through the session runtime.
    pub fn advance_segment_to(
        &mut self,
        segment: ExecutionSegment,
        requested_time: f64,
    ) -> Result<(), LiveSessionError> {
        self.session
            .advance_segment_to(segment, requested_time)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Reconcile one endpoint through the existing session publication path.
    ///
    /// The session validates that the segment reached its boundary and that any
    /// required callback phase is coherent, releases its runtime driver, and
    /// publishes the resulting authored/effective endpoint atomically. This
    /// facade retains no endpoint copy or completion state.
    pub fn complete_segment(&mut self, segment: ExecutionSegment) -> Result<(), LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .complete_segment(&mut store, segment)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Set any already-supported semantic property through one atomic publish.
    pub fn set_property(
        &mut self,
        mobject: &Mobject,
        property: SemanticObjectProperty,
        value: impl Into<SemanticSignalValue>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_property(mobject.node_id(), property, value);
        self.apply(transaction)
    }

    pub fn set_translation(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut translation = self.authored(mobject)?.transform.translation;
        translation.x = x;
        translation.y = y;
        self.set_property(mobject, SemanticObjectProperty::Translation, translation)
    }

    pub fn shift(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut translation = self.authored(mobject)?.transform.translation;
        translation.x += x;
        translation.y += y;
        self.set_property(mobject, SemanticObjectProperty::Translation, translation)
    }

    /// Shift every ordinary leaf of one semantic family in a single publication.
    ///
    /// Family traversal and alias handling stay in the shared semantic store. This
    /// is also the live-safe edit path for a detached family target: its authored
    /// leaves change without leaving the execution session on an older revision.
    pub fn shift_family(
        &mut self,
        family: &MobjectFamily,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(family)?;
        let leaves = self
            .store
            .borrow()
            .ordered_family_leaf_pairs(family.node_id(), family.node_id())
            .map_err(crate::AuthoringError::from)?
            .into_iter()
            .map(|(leaf, _)| leaf)
            .collect::<Vec<_>>();
        let mut transaction = SemanticMutationTransaction::new();
        for leaf in leaves {
            let mobject =
                Mobject::from_node(Rc::clone(self.store), leaf).map_err(LiveSessionError::from)?;
            let mut translation = self.authored(&mobject)?.transform.translation;
            translation.x += x;
            translation.y += y;
            transaction.set_property(leaf, SemanticObjectProperty::Translation, translation);
        }
        self.apply(transaction)
    }

    /// Arrange direct family members from effective runtime layout and publish
    /// every resulting leaf translation in one semantic transaction.
    pub fn arrange_family(
        &mut self,
        family: &MobjectFamily,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.arrange_family_with_options(
            family,
            &crate::FamilyArrangeOptions::new(direction_x, direction_y, buff, center),
        )
    }

    /// Stage sequential layout observations and publish one atomic family edit.
    pub fn arrange_family_with_options(
        &mut self,
        family: &MobjectFamily,
        options: &crate::FamilyArrangeOptions,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(family)?;
        self.session.require_published_store(&self.store.borrow())?;
        let plan = FamilyArrangePlan::begin(family, options).map_err(LiveSessionError::from)?;
        self.publish_family_arrangement(plan)
    }

    /// Arrange a family using coherent live bounds and one shared translation transaction.
    pub fn arrange_family_in_grid(
        &mut self,
        family: &MobjectFamily,
        rows: Option<usize>,
        columns: Option<usize>,
        gap_x: f64,
        gap_y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.arrange_family_in_grid_with_options(
            family,
            &crate::FamilyGridOptions {
                rows,
                columns,
                gap: (gap_x, gap_y),
                ..Default::default()
            },
        )
    }

    /// Grid alignment and sizing consume coherent live bounds and publish atomically.
    pub fn arrange_family_in_grid_with_options(
        &mut self,
        family: &MobjectFamily,
        options: &crate::FamilyGridOptions,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(family)?;
        self.session.require_published_store(&self.store.borrow())?;
        let plan = FamilyArrangePlan::grid(family, options).map_err(LiveSessionError::from)?;
        self.publish_family_arrangement(plan)
    }

    fn publish_family_arrangement(
        &mut self,
        mut plan: FamilyArrangePlan,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        plan.observe_leaf_bounds(|leaf| {
            let mobject = Mobject::from_node(Rc::clone(self.store), leaf)?;
            Ok::<_, LiveSessionError>(crate::family_arrangement::ArrangementBounds {
                dimensions: self.family_member_bounds(&mobject)?,
                anchors: self.family_member_measure(&mobject, true)?,
            })
        })?;
        let transaction = plan.transaction(|leaf| {
            let mobject = Mobject::from_node(Rc::clone(self.store), leaf)?;
            self.placement_authored_transform(&mobject)?;
            self.authored(&mobject).map(|s| s.transform.translation)
        })?;
        self.apply(transaction)
    }

    /// Move an object's effective layout center to one point through a single
    /// shared translation publication. Layout evaluation is bounded to this
    /// object; the caller never reconstructs geometry or an affine offset.
    pub fn move_to_point(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.move_to(
            mobject,
            LiveLayoutTarget::Point(x, y),
            (0.0, 0.0),
            (1.0, 1.0),
        )
    }

    fn placement_authored_transform(
        &self,
        mobject: &Mobject,
    ) -> Result<Transform2D, LiveSessionError> {
        let authored = self.authored(mobject)?;
        let authored_transform = Transform2D {
            translation: authored
                .transform
                .translation
                .lower_xy_f32()
                .map_err(crate::AuthoringError::from)?,
            rotation: authoring_render_f64(
                "move_to authored rotation",
                authored.transform.rotation_z,
            )
            .map_err(LiveSessionError::from)? as f32,
            scale: authored
                .transform
                .scale
                .lower_xy_f32()
                .map_err(crate::AuthoringError::from)?,
        };
        let store = self.store.borrow();
        match self
            .session
            .effective_semantic_object(&store, mobject.node_id())
        {
            Ok(observed) if !observed.authored_content_layout_applicable() => {
                return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::PlacementRenderOverride,
                )));
            }
            Ok(observed) if observed.object.transform != authored_transform => {
                return Err(LiveSessionError::from(crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver,
                )));
            }
            Ok(_) | Err(ExecutionSessionPublicationError::UnknownObject(_)) => {}
            Err(error) => return Err(error.into()),
        }
        drop(store);
        Ok(authored_transform)
    }

    /// Multiply an object's authored affine scale through the shared live
    /// transaction. The current scale is read from the semantic store, never a
    /// wrapper projection, so detached session targets follow the same path.
    pub fn scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let x = authoring_render_f64("scale.x", x).map_err(LiveSessionError::from)?;
        let y = authoring_render_f64("scale.y", y).map_err(LiveSessionError::from)?;
        let mut scale = self.authored(mobject)?.transform.scale;
        scale.x *= x;
        scale.y *= y;
        scale.lower_xy_f32().map_err(crate::AuthoringError::from)?;
        self.set_property(mobject, SemanticObjectProperty::Scale, scale)
    }

    /// Rotate about the coherent effective geometry center through the shared
    /// affine transaction. Active affine drivers must complete before this edit.
    pub fn rotate(
        &mut self,
        mobject: &Mobject,
        angle: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.rotate_layout(
            &crate::LayoutAnchor::from(mobject),
            angle,
            crate::ManimRotationPivot::Center,
        )
    }

    pub fn set_scale(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut scale = self.authored(mobject)?.transform.scale;
        scale.x = x;
        scale.y = y;
        self.set_property(mobject, SemanticObjectProperty::Scale, scale)
    }

    pub fn set_rotation(
        &mut self,
        mobject: &Mobject,
        angle: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.set_property(mobject, SemanticObjectProperty::RotationZ, angle)
    }

    pub fn replace_style(
        &mut self,
        mobject: &Mobject,
        style: SemanticStyle,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_style(mobject.node_id(), style);
        self.apply(transaction)
    }

    /// Set fill color and fill opacity through one authoritative style publication.
    pub fn set_fill(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_fill(style, red, green, blue, opacity))
    }

    pub fn set_fill_color(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| {
            edit_fill_color(style, red, green, blue, alpha)
        })
    }

    pub fn disable_fill(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| {
            edit_disable_fill(style);
            Ok(())
        })
    }

    pub fn set_fill_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_fill_opacity(style, opacity))
    }

    /// Recolor the currently enabled fill and stroke without changing their opacity.
    pub fn set_color(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_color(style, red, green, blue, alpha))
    }

    /// Set stroke color and opacity through one authoritative style publication.
    pub fn set_stroke(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| {
            edit_stroke(style, red, green, blue, opacity)
        })
    }

    pub fn set_stroke_color(
        &mut self,
        mobject: &Mobject,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| {
            edit_stroke_color(style, red, green, blue, alpha)
        })
    }

    pub fn disable_stroke(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| {
            edit_disable_stroke(style);
            Ok(())
        })
    }

    pub fn set_stroke_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_stroke_opacity(style, opacity))
    }

    /// Apply Manim's paint-opacity operation to the currently enabled paint channels.
    pub fn set_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_manim_opacity(style, opacity))
    }

    /// Set the independent object-composite opacity domain.
    pub fn set_object_opacity(
        &mut self,
        mobject: &Mobject,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(mobject, |style| edit_object_opacity(style, opacity))
    }

    /// Update supplied paint fields through one semantic publication.
    pub fn set_style(
        &mut self,
        object: &Mobject,
        update: crate::StyleUpdate,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_style(object, |style| update.apply(style))
    }

    pub fn set_family_style(
        &mut self,
        family: &MobjectFamily,
        update: crate::StyleUpdate,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_family_style(family, |style| update.apply(style))
    }

    /// Match coherent effective paint, preserving non-paint source presentation.
    pub fn match_style(
        &mut self,
        source: &Mobject,
        target: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(source)?;
        self.require_mobject(target)?;
        self.require_target_capture()?;
        let mut style = self.capture_mobject_state(source)?.style;
        let target = self.capture_mobject_state(target)?.style;
        crate::family_style::match_paint(&mut style, &target);
        self.replace_style(source, style)
    }

    pub fn match_family_style(
        &mut self,
        source: &MobjectFamily,
        target: &MobjectFamily,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(source)?;
        self.require_family(target)?;
        self.require_target_capture()?;
        let transaction = source.match_style_transaction(target, |object| {
            self.capture_mobject_state(object).map(|state| state.style)
        })?;
        self.apply(transaction)
    }

    pub fn set_family_color_by_gradient(
        &mut self,
        family: &MobjectFamily,
        colors: &[Color],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(family)?;
        self.session.require_published_store(&self.store.borrow())?;
        self.apply(
            family
                .gradient_transaction(colors)
                .map_err(LiveSessionError::from)?,
        )
    }

    pub fn set_color_by_gradient(
        &mut self,
        object: &Mobject,
        colors: &[Color],
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let colors = crate::color_gradient(colors, 1).map_err(LiveSessionError::from)?;
        let color = colors[0];
        self.set_color(
            object,
            color.red.into(),
            color.green.into(),
            color.blue.into(),
            color.alpha.into(),
        )
    }

    /// Recolor a family's unique leaves through one coherent authored publication.
    pub fn set_family_color(
        &mut self,
        family: &MobjectFamily,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_family_style(family, |style| edit_color(style, red, green, blue, alpha))
    }

    pub fn set_family_fill(
        &mut self,
        family: &MobjectFamily,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_family_style(family, |style| {
            crate::family_style::fill(style, color, opacity)
        })
    }

    pub fn set_family_stroke(
        &mut self,
        family: &MobjectFamily,
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_family_style(family, |style| {
            crate::family_style::stroke(style, color, width, opacity)
        })
    }

    pub fn set_family_opacity(
        &mut self,
        family: &MobjectFamily,
        opacity: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.edit_family_style(family, |style| edit_manim_opacity(style, opacity))
    }

    fn edit_family_style(
        &mut self,
        family: &MobjectFamily,
        edit: impl Fn(&mut SemanticStyle) -> Result<(), crate::AuthoringError>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_family(family)?;
        self.session.require_published_store(&self.store.borrow())?;
        let transaction = family
            .style_transaction(edit)
            .map_err(LiveSessionError::from)?;
        self.apply(transaction)
    }

    fn edit_style(
        &mut self,
        mobject: &Mobject,
        edit: impl FnOnce(&mut SemanticStyle) -> Result<(), crate::AuthoringError>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut style = mobject.state().map_err(LiveSessionError::from)?.style;
        edit(&mut style).map_err(LiveSessionError::from)?;
        self.replace_style(mobject, style)
    }

    fn require_mobject(&self, mobject: &Mobject) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, mobject.integration_store()) {
            return Err(LiveSessionError::ForeignMobjectStore);
        }
        mobject.validate().map_err(Into::into)
    }

    fn require_family(&self, family: &MobjectFamily) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, family.integration_store()) {
            return Err(LiveSessionError::ForeignMobjectStore);
        }
        family.validate().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_session::CallbackAdvance;
    use crate::{ExecutionSessionCreateError, Scene};
    use noon_core::{
        AnimationOptions, Color, HostCallbackId, RateFunction, SemanticPaint, SemanticVec3,
    };

    #[test]
    fn target_style_capture_rejects_resource_paint_without_a_legacy_conversion() {
        let authored = SemanticStyle {
            fill: Some(SemanticPaint::Resource(7)),
            ..SemanticStyle::default()
        };
        assert!(target_style_from_effective(&authored, Style::default()).is_err());
    }

    #[test]
    fn live_property_edits_publish_once_and_queries_are_effective_not_authored_aliases() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();

        let observed = {
            let mut live = scene.live(&mut session);
            live.set_translation(&circle, 2.0, -1.0).unwrap();
            live.set_scale(&circle, 1.5, 0.5).unwrap();
            live.set_rotation(&circle, 0.25).unwrap();
            let authored = live.authored(&circle).unwrap();
            let effective = live.effective(&circle).unwrap();
            assert_eq!(
                authored.transform.translation,
                SemanticVec3::new(2.0, -1.0, 0.0)
            );
            assert_eq!(effective.transform.translation.x, 2.0);
            assert_eq!(effective.transform.translation.y, -1.0);
            assert_eq!(effective.transform.scale.x, 1.5);
            assert_eq!(effective.transform.rotation, 0.25);
            effective
        };
        assert_eq!(observed.transform.translation.x, 2.0);
        assert_eq!(session.frame().objects.len(), 1);
    }

    #[test]
    fn live_style_edits_share_mobject_semantics_and_publish_complete_styles() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&circle).unwrap();
        let before = live.session.publication_context().scene_revision();

        live.set_fill(&target, 1.0, 0.0, 0.0, 0.4).unwrap();
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.checked_next().unwrap()
        );
        live.set_object_opacity(&target, 0.5).unwrap();
        let style = live.authored(&target).unwrap().style;
        assert_eq!(
            style.fill,
            Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0)))
        );
        assert_eq!(style.fill_opacity, 0.4);
        assert_eq!(style.object_opacity, 0.5);

        live.disable_fill(&target).unwrap();
        live.set_fill_opacity(&target, 0.25).unwrap();
        let style = live.authored(&target).unwrap().style;
        assert_eq!(style.fill, Some(SemanticPaint::Solid(Color::WHITE)));
        assert_eq!(style.fill_opacity, 0.25);

        live.set_stroke(&target, 0.0, 0.0, 1.0, 0.7).unwrap();
        live.set_color(&target, 0.0, 1.0, 0.0, 1.0).unwrap();
        let style = live.authored(&target).unwrap().style;
        assert_eq!(
            style.fill,
            Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0)))
        );
        assert_eq!(
            style.stroke,
            Some(SemanticPaint::Solid(Color::rgb(0.0, 1.0, 0.0)))
        );
        assert_eq!(style.fill_opacity, 0.25);
        assert_eq!(style.stroke_opacity, 0.7);

        live.set_opacity(&target, 0.5).unwrap();
        let style = live.authored(&target).unwrap().style;
        assert_eq!(style.fill_opacity, 0.5);
        assert_eq!(style.stroke_opacity, 0.5);
        assert_eq!(style.object_opacity, 0.5);
    }

    #[test]
    fn target_editor_captures_a_committed_callback_effective_row_without_frame_work() {
        let mut scene = Scene::new();
        let mut circle = scene.circle(1.0).unwrap();
        circle.set_fill(0.0, 0.4, 1.0, 1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut callbacks = SemanticMutationTransaction::new();
        callbacks.add_updater(circle.node_id(), HostCallbackId::new(9), 0.0, None);
        callbacks
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();

        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let revision = live.session.publication_context().scene_revision();
        let mut overlay = match live.session.advance_to_callback_barrier(0.0).unwrap() {
            CallbackAdvance::HostRequired { overlay, .. } => overlay,
            CallbackAdvance::Ready(_) => panic!("time-zero callback phase must be required"),
        };
        assert!(matches!(
            live.target_editor(&circle),
            Err(LiveSessionError::Callback(
                crate::ExecutionSessionCallbackError::Pending(_)
            ))
        ));
        assert_eq!(
            live.session.publication_context().scene_revision(),
            revision
        );

        let mut transform = overlay.object(circle.node_id()).unwrap().transform;
        transform.translation.x = 2.0;
        transform.translation.y = -1.0;
        transform.scale.x = 1.5;
        transform.rotation = 0.25;
        overlay.set_transform(circle.node_id(), transform).unwrap();
        let mut style = overlay.object(circle.node_id()).unwrap().style;
        style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.25));
        style.opacity = 0.5;
        overlay.set_style(circle.node_id(), style).unwrap();
        live.session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();

        live.session.take_frame_changes();
        let target = live.target_editor(&circle).unwrap();
        assert_eq!(live.effective(&circle).unwrap().fill_opacity(), 0.25);
        assert_eq!(target.fill_opacity().unwrap(), 0.25);
        assert_eq!(target.stroke_opacity().unwrap(), 1.0);
        let target_state = live.authored(&target).unwrap();
        assert_eq!(
            target_state.transform.translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
        assert_eq!(
            target_state.transform.scale,
            SemanticVec3::new(1.5, 1.0, 1.0)
        );
        assert_eq!(target_state.transform.rotation_z, 0.25);
        assert_eq!(
            target_state.style,
            SemanticStyle {
                fill: style.fill.map(SemanticPaint::Solid),
                fill_opacity: 1.0,
                stroke: style.stroke.map(SemanticPaint::Solid),
                stroke_opacity: 1.0,
                stroke_width: live.authored(&circle).unwrap().style.stroke_width,
                stroke_width_mode: style.stroke_width_mode,
                stroke_join: style.stroke_join,
                stroke_cap: style.stroke_cap,
                object_opacity: f64::from(style.opacity),
            }
        );
        assert!(live.session.take_frame_changes().is_empty());
        // The source's authored base remains distinct from the callback effect.
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::default()
        );
    }

    #[test]
    fn relative_affine_edits_use_shared_authored_state_for_live_and_detached_targets() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);

        live.scale(&circle, 2.0, 0.5).unwrap();
        live.rotate(&circle, 0.25).unwrap();
        let effective = live.effective(&circle).unwrap();
        assert_eq!(effective.transform.scale, noon_core::Vec2::new(2.0, 0.5));
        assert_eq!(effective.transform.rotation, 0.25);

        live.session.take_frame_changes();
        let target = live.target_editor(&circle).unwrap();
        live.scale(&target, 0.5, 4.0).unwrap();
        live.rotate(&target, 0.75).unwrap();
        let authored = live.authored(&target).unwrap();
        assert_eq!(authored.transform.scale, SemanticVec3::new(1.0, 2.0, 1.0));
        assert_eq!(authored.transform.rotation_z, 1.0);
        assert!(live.session.take_frame_changes().is_empty());
    }

    #[test]
    fn live_facade_rejects_foreign_handles_without_fallback() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let foreign = Scene::new().circle(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let live = scene.live(&mut session);
        assert!(matches!(
            live.effective(&foreign),
            Err(LiveSessionError::ForeignMobjectStore)
        ));
    }

    #[test]
    fn live_segment_drive_preserves_foreign_runtime_errors() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let segment = scene.live(&mut session).play_animation(&animation).unwrap();
        let mut foreign_runtime = session.clone();
        let before = foreign_runtime.frame().clone();

        assert!(matches!(
            scene
                .live(&mut foreign_runtime)
                .advance_segment_to(segment, segment.end_time()),
            Err(LiveSessionError::Advance(
                ExecutionSegmentAdvanceError::ForeignSegment { .. }
            ))
        ));
        assert_eq!(foreign_runtime.frame(), &before);
    }

    #[test]
    fn live_query_observes_the_active_driver_while_conflicting_edits_wait_for_completion() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();

        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();
        assert!(matches!(
            live.set_translation(&circle, 100.0, 0.0),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::SegmentCompletionPending
            ))
        ));
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::new(0.0, 0.0, 0.0)
        );
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            2.0
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        live.set_translation(&circle, 100.0, 0.0).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            100.0
        );
    }

    #[test]
    fn live_line_and_paint_queries_observe_active_drivers_without_changing_authored_state() {
        let mut scene = Scene::new();
        let mut line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        line.set_stroke_color(0.0, 0.0, 1.0, 1.0).unwrap();
        line.set_stroke_opacity(0.25).unwrap();
        let mut target = line.target_editor().unwrap();
        target.set_translation(0.0, 2.0).unwrap();
        target.set_stroke_color(1.0, 0.0, 0.0, 1.0).unwrap();
        target.set_stroke_opacity(0.75).unwrap();
        scene.add(&line).unwrap();
        let animation = scene
            .declare_transform_to(
                &line,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();

        assert_eq!(
            line.manim_line_endpoints().unwrap(),
            ManimLineEndpoints {
                start: (-1.0, 0.0),
                end: (1.0, 0.0),
            }
        );
        assert_eq!(
            live.effective_line_endpoints(&line).unwrap(),
            ManimLineEndpoints {
                start: (-1.0, 1.0),
                end: (1.0, 1.0),
            }
        );
        assert_eq!(line.manim_color().unwrap(), Color::rgb(0.0, 0.0, 1.0));
        assert_eq!(
            live.effective_manim_color(&line).unwrap(),
            Color::rgb(0.5, 0.0, 0.5)
        );
        let effective = live.effective(&line).unwrap();
        assert_eq!(effective.fill_opacity(), 0.0);
        assert_eq!(effective.stroke_opacity(), 0.5);
        assert_eq!(line.stroke_opacity().unwrap(), 0.25);
    }

    #[test]
    fn live_line_endpoints_reject_active_content_overrides() {
        let scene = Scene::new();
        let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        live.declare_and_activate_create(
            &line,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )
        .unwrap();

        assert!(live.effective_line_endpoints(&line).is_err());
        assert_eq!(live.effective_manim_color(&line).unwrap(), Color::WHITE);
    }

    #[test]
    fn effective_layout_uses_shared_layout_bounds_without_stroke_expansion() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, -2.0).unwrap();
        scene.add(&circle).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();

        let layout = live.effective_layout(&circle).unwrap();
        assert_eq!(layout.center, (2.0, -1.0));
        assert_eq!((layout.width, layout.height), (2.0, 2.0));
    }

    #[test]
    fn move_to_point_uses_effective_layout_and_publishes_one_local_translation() {
        let mut scene = Scene::new();
        // This line's geometric center is offset from its authored translation,
        // so setting translation directly would not implement MoveTo semantics.
        let line = scene.line((0.0, 0.0), (2.0, 0.0)).unwrap();
        scene.add(&line).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);

        assert_eq!(live.effective_layout(&line).unwrap().center, (1.0, 0.0));
        let result = live.move_to_point(&line, 5.0, -3.0).unwrap();

        assert_eq!(result.impacts().len(), 1);
        assert_eq!(
            live.authored(&line).unwrap().transform.translation,
            SemanticVec3::new(4.0, -3.0, 0.0)
        );
        assert_eq!(live.effective_layout(&line).unwrap().center, (5.0, -3.0));
    }

    #[test]
    fn move_to_point_edits_a_detached_target_without_execution_enrollment() {
        let mut scene = Scene::new();
        let frame = scene.rectangle(4.0, 2.0).unwrap();
        scene.add(&frame).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let target = live.target_editor(&frame).unwrap();
        live.session.take_frame_changes();

        let result = live.move_to_point(&target, 3.0, -2.0).unwrap();

        assert_eq!(result.impacts().len(), 1);
        assert_eq!(
            live.authored(&target).unwrap().transform.translation,
            SemanticVec3::new(3.0, -2.0, 0.0)
        );
        assert_eq!(target.center().unwrap(), (3.0, -2.0));
        assert!(matches!(
            live.effective(&target),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::UnknownObject(_)
            ))
        ));
        assert_eq!(live.session.frame().objects.len(), 1);
        assert!(live.session.take_frame_changes().is_empty());
    }

    #[test]
    fn move_to_uses_shared_edges_masks_and_atomic_detached_target_edits() {
        let mut scene = Scene::new();
        let mut source = scene.rectangle(4.0, 2.0).unwrap();
        source.set_translation(2.0, -1.0).unwrap();
        let mut reference = scene.rectangle(2.0, 4.0).unwrap();
        reference.set_translation(-3.0, 3.0).unwrap();
        scene.add(&source).unwrap();
        scene.add(&reference).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        live.session.take_frame_changes();
        let result = live
            .move_to(
                &source,
                LiveLayoutTarget::Point(99.0, 5.0),
                (0.0, 1.0),
                (0.0, 1.0),
            )
            .unwrap();
        assert_eq!(result.impacts().len(), 1);
        assert_eq!(live.effective_layout(&source).unwrap().center, (2.0, 4.0));
        assert_eq!(
            live.effective_layout(&reference).unwrap().center,
            (-3.0, 3.0)
        );
        let target = live.target_editor(&source).unwrap();
        live.session.take_frame_changes();
        live.move_to(
            &target,
            LiveLayoutTarget::Mobject(&reference),
            (0.0, 1.0),
            (0.5, 1.0),
        )
        .unwrap();
        assert_eq!(target.center().unwrap(), (-0.5, 4.0));
        assert_eq!(live.effective_layout(&source).unwrap().center, (2.0, 4.0));
        assert!(live.session.take_frame_changes().is_empty());
        let publication = live.session.publication_context();
        assert!(live
            .move_to(
                &target,
                LiveLayoutTarget::Point(1.0, 2.0),
                (0.0, 0.0),
                (f64::NAN, 1.0)
            )
            .is_err());
        let foreign = Scene::new().circle(1.0).unwrap();
        assert!(live
            .move_to(
                &target,
                LiveLayoutTarget::Mobject(&foreign),
                (0.0, 0.0),
                (1.0, 1.0)
            )
            .is_err());
        assert_eq!(live.session.publication_context(), publication);
        assert_eq!(target.center().unwrap(), (-0.5, 4.0));
    }

    #[test]
    fn move_to_point_rejects_an_active_affine_driver_before_publication() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let animation = scene
            .declare_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live.play_animation(&animation).unwrap();
        live.advance_segment_to(segment, 1.0).unwrap();
        let before = live.session.publication_context();

        assert!(matches!(
            live.move_to_point(&circle, 3.0, 0.0),
            Err(LiveSessionError::Authoring(
                crate::AuthoringError::Unsupported(
                    crate::UnsupportedAuthoringOperation::PlacementEffectiveAffineDriver
                )
            ))
        ));
        assert_eq!(live.session.publication_context(), before);
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::ZERO
        );
    }

    #[test]
    fn live_geometry_creation_is_detached_atomic_and_admits_locally() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let before = live.session.publication_context();
        live.session.take_frame_changes();

        let mut invalid = crate::ManimGeometryOptions::circle(0.25).unwrap();
        assert!(invalid.set_stroke_width(-0.1).is_err());
        assert_eq!(live.session.publication_context(), before);
        assert!(live.session.take_frame_changes().is_empty());

        let mut options = crate::ManimGeometryOptions::circle(0.25).unwrap();
        options.set_translation(2.0, -1.0).unwrap();
        options.set_fill(0.0, 0.4, 1.0, 0.6).unwrap();
        let circle = live.create_manim_geometry(options).unwrap();
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        let authored = live.authored(&circle).unwrap();
        assert_eq!(
            authored.transform.translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
        assert_eq!(authored.style.fill_opacity, 0.6);
        assert!(matches!(
            live.effective(&circle),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::UnknownObject(_)
            ))
        ));
        assert_eq!(live.session.frame().objects.len(), 1);
        assert!(live.session.take_frame_changes().is_empty());

        live.set_translation(&circle, 2.0, -1.0).unwrap();
        assert!(live.session.take_frame_changes().is_empty());
        live.add(&circle).unwrap();
        assert_eq!(live.session.frame().objects.len(), 2);
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            2.0
        );
    }

    #[test]
    fn live_self_become_preserves_precise_authored_state_without_publication() {
        let mut scene = Scene::new();
        let mut source = scene.circle(0.5).unwrap();
        source
            .set_translation(0.123_456_789_012, -0.234_567_890_123)
            .unwrap();
        source.set_fill_opacity(0.345_678_901_234).unwrap();
        source.set_object_opacity(0.456_789_012_345).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before_state = source.state().unwrap();
        let before_publication = session.publication_context();
        let mut live = scene.live(&mut session);

        live.become_mobject(&source, &source, crate::ManimBecomeOptions::default())
            .unwrap();

        assert_eq!(source.state().unwrap(), before_state);
        assert_eq!(live.session.publication_context(), before_publication);
        assert!(live.session.take_frame_changes().is_empty());
    }

    #[test]
    fn stale_live_geometry_rejects_before_importing_path_resources() {
        use noon_core::{Vec2, VectorPath};
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before_resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let path = crate::ManimGeometryOptions::path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 1.0)),
        )
        .unwrap();

        Mobject::manim_circle(Rc::clone(scene.integration_store()), 0.1).unwrap();
        let mut live = scene.live(&mut session);
        assert!(matches!(
            live.create_manim_geometry(path),
            Err(LiveSessionError::Publication(
                ExecutionSessionPublicationError::StaleSceneRevision { .. }
            ))
        ));
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            before_resources
        );
    }

    #[test]
    fn returning_transform_completion_preserves_the_source_across_activation_paths_and_nested_timing(
    ) {
        for mode in ["predeclared", "prepared", "mapped"] {
            let mut scene = Scene::new();
            let mut source = scene.circle(0.4).unwrap();
            source.set_translation(1.25, -0.75).unwrap();
            source.set_fill(0.1, 0.2, 0.3, 0.4).unwrap();
            let mut target = source.target_editor().unwrap();
            target.set_translation(3.25, 1.25).unwrap();
            target.set_fill(0.8, 0.7, 0.6, 0.9).unwrap();
            scene.add(&source).unwrap();
            let original = source.state().unwrap();
            let options = AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::ThereAndBack);
            let animation = if mode != "predeclared" {
                None
            } else {
                Some(
                    scene
                        .declare_transform_to(&source, &target, options)
                        .unwrap(),
                )
            };
            let mut session = scene.execution_session().unwrap();
            let mut live = scene.live(&mut session);
            let segment = if let Some(animation) = animation {
                live.play_animation(&animation).unwrap()
            } else if mode == "mapped" {
                live.declare_and_activate_transform_composition(
                    SemanticAnimationCompositionKind::Parallel,
                    &[TransformToRequest::new(
                        &source,
                        &target,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    )],
                    options,
                    AnimationOptions::new(),
                )
                .unwrap()
            } else {
                live.declare_and_activate_transform_to(&source, &target, options)
                    .unwrap()
            };
            live.advance_segment_to(segment, 0.5).unwrap();
            assert_eq!(
                live.effective(&source).unwrap().transform.translation,
                noon_core::Vec2::new(3.25, 1.25)
            );
            live.advance_segment_to(segment, 1.0).unwrap();
            let before_completion = live.effective(&source).unwrap();
            live.complete_segment(segment).unwrap();
            let mut expected = original;
            if mode == "mapped" {
                let target_state = target.state().unwrap();
                expected.transform = target_state.transform;
                expected.style = target_state.style;
            }
            assert_eq!(live.authored(&source).unwrap(), expected);
            let after_completion = live.effective(&source).unwrap();
            assert_eq!(after_completion.transform, before_completion.transform);
            assert_eq!(after_completion.style, before_completion.style);
            live.set_translation(&source, -2.0, 0.0).unwrap();
            assert_eq!(
                live.effective(&source).unwrap().transform.translation.x,
                -2.0
            );
        }
    }

    #[test]
    fn returning_sequence_completion_keeps_the_preceding_leaf_endpoint() {
        let mut scene = Scene::new();
        let source = scene.circle(0.4).unwrap();
        let mut first = source.target_editor().unwrap();
        first.set_translation(2.0, 1.0).unwrap();
        first.set_fill(0.2, 0.3, 0.4, 0.5).unwrap();
        let mut second = first.target_editor().unwrap();
        second.set_translation(4.0, 3.0).unwrap();
        second.set_fill(0.8, 0.7, 0.6, 0.9).unwrap();
        scene.add(&source).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Sequence,
                &[
                    TransformToRequest::new(
                        &source,
                        &first,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::Linear),
                    ),
                    TransformToRequest::new(
                        &source,
                        &second,
                        AnimationOptions::new()
                            .run_time(1.0)
                            .rate_func(RateFunction::ThereAndBack),
                    ),
                ],
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        live.advance_segment_to(segment, 1.5).unwrap();
        assert_eq!(
            live.effective(&source).unwrap().transform.translation,
            noon_core::Vec2::new(4.0, 3.0)
        );
        live.advance_segment_to(segment, 2.0).unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.authored(&source).unwrap().transform,
            first.state().unwrap().transform
        );
        assert_eq!(
            live.authored(&source).unwrap().style,
            first.state().unwrap().style
        );
        assert_eq!(
            live.effective(&source).unwrap().transform.translation,
            noon_core::Vec2::new(2.0, 1.0)
        );
    }

    #[test]
    fn completion_reconciles_the_endpoint_before_the_next_live_segment() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut first_target = circle.target_editor().unwrap();
        first_target.set_translation(2.0, -2.0).unwrap();
        let mut second_target = circle.target_editor().unwrap();
        second_target.set_translation(5.0, -2.0).unwrap();
        scene.add(&circle).unwrap();
        let first = scene
            .declare_transform_to(
                &circle,
                &first_target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let second = scene
            .declare_transform_to(
                &circle,
                &second_target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);

        let first_segment = live.play_animation(&first).unwrap();
        live.advance_segment_to(first_segment, first_segment.end_time())
            .unwrap();
        assert!(!live.segment_state(first_segment).is_complete());
        live.complete_segment(first_segment).unwrap();
        assert!(live.segment_state(first_segment).is_complete());
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            2.0
        );

        live.set_translation(&circle, 3.0, -2.0).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            3.0
        );
        let second_segment = live.play_animation(&second).unwrap();
        live.advance_segment_to(second_segment, second_segment.end_time())
            .unwrap();
        live.complete_segment(second_segment).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            5.0
        );
    }

    #[test]
    fn post_bootstrap_transform_declaration_and_activation_publish_atomically() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(8.0, -2.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(segment.start_time(), 0.0);
        assert_eq!(segment.end_time(), 2.0);
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );

        live.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            4.0
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::new(8.0, -2.0, 0.0)
        );
    }

    #[test]
    fn parallel_create_admits_all_detached_leaves_in_one_reveal_segment() {
        let scene = Scene::new();
        let circle = scene.circle(0.4).unwrap();
        let square = scene.square(0.8).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = circle.integration_store().borrow().len();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Smooth);

        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_create_parallel(
                &[(&circle, options), (&square, options)],
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(segment.start_time(), 0.0);
        assert_eq!(segment.end_time(), 1.0);
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        // Two Create leaves and one Parallel root share one semantic publication.
        assert_eq!(circle.integration_store().borrow().len(), before_nodes + 3);
        assert!(live.contains(&circle).unwrap());
        assert!(live.contains(&square).unwrap());
        assert_eq!(live.session.frame().objects.len(), 2);
        assert!(!live.segment_state(segment).is_complete());

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert!(live.segment_state(segment).is_complete());
        assert_eq!(live.session.frame().objects.len(), 2);
    }

    #[test]
    fn parallel_create_rejects_duplicate_detached_target_before_publication() {
        let scene = Scene::new();
        let circle = scene.circle(0.4).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = circle.integration_store().borrow().len();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);

        let result = scene
            .live(&mut session)
            .declare_and_activate_create_parallel(
                &[(&circle, options), (&circle, options)],
                AnimationOptions::new().run_time(1.0),
            );

        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::CreateTarget {
                    error: ExecutionSessionCreateError::DuplicateTarget,
                    ..
                }
            ))
        ));
        assert_eq!(session.publication_context(), before);
        assert_eq!(circle.integration_store().borrow().len(), before_nodes);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn affine_lifecycle_admits_from_effective_channels_then_removes_at_completion() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let authored = square.state().unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);

        let grow = live
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::IntroduceFrom,
                AffineLifecycleEndpoint::Point {
                    x: -2.0,
                    y: 1.0,
                    rotation_offset: -std::f64::consts::FRAC_PI_2,
                    point_color: Some(Color::RED),
                },
                options,
            )
            .unwrap();
        assert!(live.contains(&square).unwrap());
        let start = live.effective(&square).unwrap();
        assert_eq!(start.transform.translation, noon_core::Vec2::new(-2.0, 1.0));
        assert_eq!(start.transform.scale, noon_core::Vec2::ZERO);
        live.advance_segment_to(grow, grow.end_time()).unwrap();
        live.complete_segment(grow).unwrap();
        assert_eq!(live.authored(&square).unwrap(), authored);

        let shrink = live
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::RemoveTo,
                AffineLifecycleEndpoint::EffectiveCenter,
                options,
            )
            .unwrap();
        live.advance_segment_to(shrink, shrink.end_time()).unwrap();
        assert!(live.contains(&square).unwrap());
        live.complete_segment(shrink).unwrap();
        assert!(!live.contains(&square).unwrap());
        assert_eq!(square.state().unwrap(), authored);
    }

    #[test]
    fn detached_effective_center_removal_admits_and_removes_one_identity_atomically() {
        let scene = Scene::new();
        let mut square = scene.square(1.0).unwrap();
        square.set_translation(2.0, -1.0).unwrap();
        let detached_family = {
            let mut store = scene.integration_store().borrow_mut();
            let family = store.insert_family();
            store.add_member(family, square.node_id()).unwrap();
            family
        };
        let semantic_id = square.node_id();
        let authored = square.state().unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before_revision = session.publication_context().scene_revision();
        let mut live = scene.live(&mut session);

        let segment = live
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::RemoveTo,
                AffineLifecycleEndpoint::EffectiveCenter,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(square.node_id(), semantic_id);
        assert!(live.contains(&square).unwrap());
        assert_eq!(live.authored(&square).unwrap(), authored);
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before_revision.checked_next().unwrap()
        );

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert!(live.contains(&square).unwrap());
        live.complete_segment(segment).unwrap();
        assert!(!live.contains(&square).unwrap());
        assert_eq!(square.node_id(), semantic_id);
        assert_eq!(square.state().unwrap(), authored);
        let store = square.integration_store().borrow();
        let parents = store.node(square.node_id()).unwrap().parents();
        assert_eq!(parents.len(), 1);
        assert!(parents.contains(&detached_family));
    }

    #[test]
    fn grow_then_shrink_preserves_an_unmounted_family_membership() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let detached_family = {
            let mut store = scene.integration_store().borrow_mut();
            let family = store.insert_family();
            store.add_member(family, square.node_id()).unwrap();
            family
        };
        let semantic_id = square.node_id();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let endpoint = AffineLifecycleEndpoint::Point {
            x: 0.0,
            y: 0.0,
            rotation_offset: 0.0,
            point_color: None,
        };

        let segment = live
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::IntroduceFrom,
                endpoint,
                options,
            )
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(square.node_id(), semantic_id);
        {
            let store = square.integration_store().borrow();
            let parents = store.node(square.node_id()).unwrap().parents();
            assert_eq!(parents.len(), 2);
            assert!(parents.contains(&detached_family));
            assert!(parents.contains(&scene.root()));
        }

        let before = live.session.publication_context();
        let result = live.declare_and_activate_affine_lifecycle(
            &square,
            AffineLifecycleDirection::IntroduceFrom,
            endpoint,
            options,
        );
        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::FadeTarget {
                    error: crate::ExecutionSessionFadeError::TargetIsNotDetached,
                    ..
                }
            ))
        ));
        assert_eq!(live.session.publication_context(), before);

        let foreign = Scene::new().square(1.0).unwrap();
        assert!(matches!(
            live.declare_and_activate_affine_lifecycle(
                &foreign,
                AffineLifecycleDirection::IntroduceFrom,
                endpoint,
                options,
            ),
            Err(LiveSessionError::ForeignMobjectStore)
        ));
        assert_eq!(live.session.publication_context(), before);

        let shrink = live
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::RemoveTo,
                AffineLifecycleEndpoint::EffectiveCenter,
                options,
            )
            .unwrap();
        live.advance_segment_to(shrink, shrink.end_time()).unwrap();
        live.complete_segment(shrink).unwrap();
        assert!(!live.contains(&square).unwrap());
        let store = square.integration_store().borrow();
        let parents = store.node(square.node_id()).unwrap().parents();
        assert_eq!(parents.len(), 1);
        assert!(parents.contains(&detached_family));
    }

    #[test]
    fn affine_removal_rejects_another_live_reachable_parent() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let live_family = {
            let mut store = scene.integration_store().borrow_mut();
            let family = store.insert_family();
            store.add_member(family, square.node_id()).unwrap();
            family
        };
        // Build an intentionally aliased root to exercise removal preflight;
        // standard membership authoring dissolves this redundant projection.
        let mut membership = SemanticMutationTransaction::new();
        membership.add_member(scene.root(), live_family);
        membership.add_member(scene.root(), square.node_id());
        membership
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        let result = scene
            .live(&mut session)
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::RemoveTo,
                AffineLifecycleEndpoint::EffectiveCenter,
                AnimationOptions::new().run_time(1.0),
            );
        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::FadeTarget {
                    error: crate::ExecutionSessionFadeError::TargetIsAliased,
                    ..
                }
            ))
        ));
        assert_eq!(session.publication_context(), before);
    }

    #[test]
    fn invalid_detached_affine_removal_does_not_admit_or_publish() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = square.integration_store().borrow().len();

        let result = scene
            .live(&mut session)
            .declare_and_activate_affine_lifecycle(
                &square,
                AffineLifecycleDirection::RemoveTo,
                AffineLifecycleEndpoint::Point {
                    x: f64::NAN,
                    y: 0.0,
                    rotation_offset: 0.0,
                    point_color: None,
                },
                AnimationOptions::new().run_time(1.0),
            );

        assert!(matches!(result, Err(LiveSessionError::Activation(_))));
        assert_eq!(session.publication_context(), before);
        assert_eq!(square.integration_store().borrow().len(), before_nodes);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn uncreate_rejects_foreign_detached_target_without_publication() {
        let scene = Scene::new();
        let foreign = Scene::new().square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        let result = scene
            .live(&mut session)
            .declare_and_activate_uncreate(&foreign, AnimationOptions::new().run_time(1.0));

        assert!(matches!(result, Err(LiveSessionError::ForeignMobjectStore)));
        assert_eq!(session.publication_context(), before);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn uncreate_reverses_and_removes_a_direct_bound_leaf() {
        let mut scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let semantic_id = square.node_id();
        let authored = square.state().unwrap();
        scene.add(&square).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();

        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_uncreate(
                &square,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert!(live.contains(&square).unwrap());
        assert_eq!(live.session.frame().reveal(0), 1.0);

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert_eq!(live.session.frame().reveal(0), 0.0);
        live.complete_segment(segment).unwrap();
        assert!(!live.contains(&square).unwrap());
        assert_eq!(square.node_id(), semantic_id);
        assert_eq!(square.state().unwrap(), authored);
    }

    #[test]
    fn uncreate_honors_asymmetric_reversal_without_removing_kept_target() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_uncreate(
                &square,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::RushInto)
                    .remover(false),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 0.25)
            .unwrap();
        assert!(
            (live.session.frame().reveal(0) - RateFunction::RushInto.evaluate(0.75)).abs() < 1e-6
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        assert_eq!(live.session.frame().reveal(0), 0.0);
        live.complete_segment(segment).unwrap();
        assert!(live.contains(&square).unwrap());
        assert_eq!(live.session.frame().reveal(0), 0.0);
    }

    #[test]
    fn uncreate_honors_explicit_forward_rate_and_keeps_membership() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_uncreate(
                &square,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::RushInto)
                    .reverse_rate_function(false)
                    .remover(false),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 0.25)
            .unwrap();
        assert!(
            (live.session.frame().reveal(0) - RateFunction::RushInto.evaluate(0.25)).abs() < 1e-6
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert!(live.contains(&square).unwrap());
        assert_eq!(live.session.frame().reveal(0), 1.0);
    }

    #[test]
    fn prepared_parallel_composition_publishes_one_revision_and_completes_both_leaves() {
        let mut scene = Scene::new();
        let mut left = scene.circle(1.0).unwrap();
        left.set_translation(-2.0, 0.0).unwrap();
        let mut right = scene.circle(1.0).unwrap();
        right.set_translation(2.0, 0.0).unwrap();
        let mut left_target = left.target_editor().unwrap();
        left_target.set_translation(-2.0, 1.0).unwrap();
        let mut right_target = right.target_editor().unwrap();
        right_target.set_translation(2.0, -1.0).unwrap();
        scene.add(&left).unwrap();
        scene.add(&right).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = left.integration_store().borrow().len();

        let children = [
            TransformToRequest::new(
                &left,
                &left_target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            ),
            TransformToRequest::new(
                &right,
                &right_target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            ),
        ];
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();

        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        // Two immutable target snapshots, two leaves, and one root share that commit.
        assert_eq!(left.integration_store().borrow().len(), before_nodes + 5);
        let publication = live.session.last_structural_publication_stats();
        assert_eq!(publication.preparation.object_states_lowered, 0);
        assert_eq!(publication.entered_objects, 0);
        assert_eq!(publication.exited_objects, 0);
        live.advance_segment_to(segment, 1.0).unwrap();
        assert_eq!(
            live.effective(&left).unwrap().transform.translation,
            noon_core::Vec2::new(-2.0, 0.5)
        );
        assert_eq!(
            live.effective(&right).unwrap().transform.translation,
            noon_core::Vec2::new(2.0, -0.5)
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.authored(&left).unwrap().transform.translation,
            SemanticVec3::new(-2.0, 1.0, 0.0)
        );
        assert_eq!(
            live.authored(&right).unwrap().transform.translation,
            SemanticVec3::new(2.0, -1.0, 0.0)
        );
    }

    #[test]
    fn prepared_sequence_captures_composed_style_targets_at_mapped_boundaries() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut fill_target = circle.target_editor().unwrap();
        fill_target.set_fill(1.0, 0.0, 0.0, 0.4).unwrap();
        // TransformTo targets are complete snapshots. Carry the first target's
        // paint into the second target while changing its opacity.
        let mut opacity_target = fill_target.target_editor().unwrap();
        opacity_target.set_object_opacity(0.5).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let children = [
            TransformToRequest::new(
                &circle,
                &fill_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
            TransformToRequest::new(
                &circle,
                &opacity_target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            ),
        ];
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Sequence,
                &children,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();

        live.advance_segment_to(segment, 1.0).unwrap();
        let boundary = live.effective(&circle).unwrap().style;
        assert_eq!(boundary.fill, Some(Color::rgba(1.0, 0.0, 0.0, 0.4)));
        assert_eq!(boundary.opacity, 1.0);

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        let endpoint = live.effective(&circle).unwrap().style;
        assert_eq!(endpoint.fill, Some(Color::rgba(1.0, 0.0, 0.0, 0.4)));
        assert_eq!(endpoint.opacity, 0.5);
        live.complete_segment(segment).unwrap();
        let authored = live.authored(&circle).unwrap().style;
        assert_eq!(
            authored.fill,
            Some(SemanticPaint::Solid(Color::rgb(1.0, 0.0, 0.0)))
        );
        assert_eq!(authored.fill_opacity, 0.4);
        assert_eq!(authored.object_opacity, 0.5);
    }

    #[test]
    fn sequential_transform_targets_capture_previous_effective_endpoints() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut first = circle.target_editor().unwrap();
        first.set_translation(2.0, 1.0).unwrap();
        let mut second = circle.target_editor().unwrap();
        second.set_translation(4.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let children = [
            TransformToRequest::new(&circle, &first, options),
            TransformToRequest::new(&circle, &second, options),
        ];
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Sequence,
                &children,
                AnimationOptions::new().rate_func(RateFunction::Linear),
                AnimationOptions::new(),
            )
            .unwrap();
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        for (time, expected) in [
            (0.5, noon_core::Vec2::new(1.0, 0.5)),
            (1.0, noon_core::Vec2::new(2.0, 1.0)),
            (1.5, noon_core::Vec2::new(3.0, 0.5)),
            (2.0, noon_core::Vec2::new(4.0, 0.0)),
        ] {
            live.advance_segment_to(segment, time).unwrap();
            assert_eq!(
                live.effective(&circle).unwrap().transform.translation,
                expected
            );
        }
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::new(4.0, 0.0, 0.0)
        );
    }

    #[test]
    fn duplicate_composition_driver_rolls_back_target_leaf_and_root_declarations() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut first_target = circle.target_editor().unwrap();
        first_target.set_translation(1.0, 0.0).unwrap();
        let mut second_target = circle.target_editor().unwrap();
        second_target.set_translation(2.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_frame = session.frame().clone();
        let before_nodes = circle.integration_store().borrow().len();
        let children = [
            TransformToRequest::new(&circle, &first_target, AnimationOptions::new()),
            TransformToRequest::new(&circle, &second_target, AnimationOptions::new()),
        ];

        let result = scene
            .live(&mut session)
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new(),
                AnimationOptions::new().run_time(2.0),
            );

        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::PreparedAnimation(
                    noon_compile::PreparedSemanticAnimationLoweringError::MultipleDrivers { .. }
                )
            ))
        ));
        assert_eq!(session.publication_context(), before);
        assert_eq!(session.frame(), &before_frame);
        assert_eq!(circle.integration_store().borrow().len(), before_nodes);
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn mixed_composition_rejects_foreign_leaf_before_detached_admission() {
        let scene = Scene::new();
        let square = scene.rectangle(2.0, 2.0).unwrap();
        let mut target = square.target_editor().unwrap();
        target.rotate(std::f64::consts::PI).unwrap();
        let foreign = Scene::new().rectangle(2.0, 2.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_frame = session.frame().clone();
        let before_nodes = square.integration_store().borrow().len();
        let children = [
            AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                &square,
                &target,
                AnimationOptions::new(),
            )),
            AnimationCompositionRequest::Rotate {
                target: &foreign,
                angle: std::f64::consts::PI,
                options: AnimationOptions::new(),
            },
        ];

        let result = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &children,
                AnimationOptions::new(),
                AnimationOptions::new().run_time(2.0),
            );

        assert!(matches!(result, Err(LiveSessionError::ForeignMobjectStore)));
        assert_eq!(session.publication_context(), before);
        assert_eq!(session.frame(), &before_frame);
        assert_eq!(square.integration_store().borrow().len(), before_nodes);
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn unsupported_point_correspondence_rolls_back_detached_admission() {
        let scene = Scene::new();
        let line = scene.line((-1.0, 0.0), (1.0, 0.0)).unwrap();
        let mut target = line.target_editor().unwrap();
        target.rotate(std::f64::consts::PI).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = line.integration_store().borrow().len();

        let result = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Parallel,
                &[AnimationCompositionRequest::TransformTo(
                    TransformToRequest::point_correspondence(
                        &line,
                        &target,
                        AnimationOptions::new(),
                    ),
                )],
                AnimationOptions::new(),
                AnimationOptions::new(),
            );

        assert!(matches!(
            &result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::PreparedAnimation(
                    noon_compile::PreparedSemanticAnimationLoweringError::UnsupportedPointCorrespondence { .. }
                )
            ))
        ), "unexpected rejection: {result:?}");
        assert_eq!(session.publication_context(), before);
        assert_eq!(line.integration_store().borrow().len(), before_nodes);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn mixed_sequence_preserves_rotate_before_transform_order() {
        let scene = Scene::new();
        let rotating = scene.square(1.0).unwrap();
        let moving = scene.square(1.0).unwrap();
        let mut moving_target = moving.target_editor().unwrap();
        moving_target.set_translation(2.0, 0.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let segment = scene
            .live(&mut session)
            .declare_and_activate_animation_composition(
                SemanticAnimationCompositionKind::Sequence,
                &[
                    AnimationCompositionRequest::Rotate {
                        target: &rotating,
                        angle: std::f64::consts::PI,
                        options,
                    },
                    AnimationCompositionRequest::TransformTo(TransformToRequest::new(
                        &moving,
                        &moving_target,
                        options,
                    )),
                ],
                AnimationOptions::new().lag_ratio(1.0),
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();
        let mut live = scene.live(&mut session);
        live.advance_segment_to(segment, 0.5).unwrap();
        assert!(
            (live.effective(&rotating).unwrap().transform.rotation - std::f32::consts::FRAC_PI_2)
                .abs()
                < 1e-5
        );
        assert_eq!(
            live.effective(&moving).unwrap().transform.translation,
            noon_core::Vec2::ZERO
        );
        live.advance_segment_to(segment, 1.5).unwrap();
        assert!(
            (live.effective(&rotating).unwrap().transform.rotation - std::f32::consts::PI).abs()
                < 1e-5
        );
        assert_eq!(
            live.effective(&moving).unwrap().transform.translation,
            noon_core::Vec2::new(1.0, 0.0)
        );
    }

    #[test]
    fn invalid_or_conflicting_post_bootstrap_activation_does_not_publish() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut target = circle.target_editor().unwrap();
        target.set_translation(3.0, 0.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();

        let before = session.publication_context();
        let before_frame = session.frame().clone();
        let invalid = scene.live(&mut session).declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new().run_time(f64::NAN),
        );
        assert!(matches!(
            invalid,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::AuthoredPublication(
                    ExecutionSessionPublicationError::Semantic(
                        noon_core::SemanticMutationTransactionError::InvalidAnimationRunTime { .. }
                    )
                )
            ))
        ));
        assert_eq!(session.publication_context(), before);
        assert_eq!(session.frame(), &before_frame);
        assert!(session.take_frame_changes().is_empty());

        let segment = scene
            .live(&mut session)
            .declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        let published = session.publication_context();
        let frame = session.frame().clone();
        let rejected = scene.live(&mut session).declare_and_activate_transform_to(
            &circle,
            &target,
            AnimationOptions::new().run_time(1.0),
        );
        assert!(matches!(
            rejected,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::SegmentCompletionPending
            ))
        ));
        assert_eq!(session.publication_context(), published);
        assert_eq!(session.frame(), &frame);
        assert!(!session.segment_state(segment).is_complete());
    }

    #[test]
    fn live_target_created_after_wait_stays_in_the_same_publication_chain() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        scene.add(&circle).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);

        let wait = live.wait_segment(3.0).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        assert_eq!(live.session.frame().time, 3.0);
        let target = live.target_editor(&circle).unwrap();
        live.set_translation(&target, 6.0, 1.0).unwrap();
        assert!(live.session.take_frame_changes().is_empty());
        let segment = live
            .declare_and_activate_transform_to(
                &circle,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(segment.start_time(), 3.0);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.effective(&circle).unwrap().transform.translation.x,
            6.0
        );
    }

    #[test]
    fn detached_target_editor_after_wait_uses_authored_state_and_enters_with_transform() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        let mut square = scene.square(1.0).unwrap();
        square.set_translation(-2.0, 0.5).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);

        let wait = live.wait_segment(1.0).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        let target = live.target_editor(&square).unwrap();
        live.set_translation(&target, 3.0, -1.0).unwrap();
        let request = AnimationCompositionRequest::TransformTo(TransformToRequest::new(
            &square,
            &target,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        ));
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();

        assert!(live.contains(&square).unwrap());
        assert_eq!(segment.start_time(), 1.0);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.effective(&square).unwrap().transform.translation,
            noon_core::Vec2::new(3.0, -1.0)
        );
    }

    #[test]
    fn live_membership_detaches_readds_and_appends_without_changing_unrelated_slots() {
        let mut scene = Scene::new();
        let anchor = scene.circle(1.0).unwrap();
        let toggled = scene.circle(2.0).unwrap();
        let detached = scene.circle(3.0).unwrap();
        scene.add(&anchor).unwrap();
        scene.add(&toggled).unwrap();
        let mut session = scene.execution_session().unwrap();
        let anchor_slot = session.execution_slot_for_frame_index(0).unwrap();

        {
            let mut live = scene.live(&mut session);
            live.remove(&toggled).unwrap();
            assert!(live.effective(&toggled).is_err());
            live.add(&toggled).unwrap();
            assert!(live.effective(&toggled).is_ok());
            live.add(&detached).unwrap();
            assert_eq!(
                live.session
                    .last_structural_publication_stats()
                    .entered_objects,
                1
            );
            live.set_translation(&detached, 4.0, -2.0).unwrap();
            assert_eq!(
                live.effective(&detached).unwrap().transform.translation,
                noon_core::Vec2::new(4.0, -2.0)
            );
        }

        assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));
        assert_eq!(session.frame().objects.len(), 3);
    }

    #[test]
    fn raster_lagged_fades_of_late_detached_family_members() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.3).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(1.0).unwrap();
        live.advance_segment_to(wait, 1.0).unwrap();
        live.complete_segment(wait).unwrap();
        let first = live
            .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
            .unwrap();
        let second = live
            .create_manim_geometry(crate::ManimGeometryOptions::square(0.7).unwrap())
            .unwrap();
        let _family = live
            .family(&[
                MobjectFamilyMember::Mobject(&first),
                MobjectFamilyMember::Mobject(&second),
            ])
            .unwrap();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new()
                .run_time(2.2)
                .rate_func(RateFunction::Linear)
                .lag_ratio(0.1),
            children: [&first, &second]
                .into_iter()
                .map(|target| AnimationCompositionRequest::Fade {
                    target,
                    direction: SemanticFadeDirection::In,
                    endpoint: FadeEndpoint::default(),
                    options: AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                })
                .collect(),
        };
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        for index in 30..=96 {
            live.advance_segment_to(segment, f64::from(index) / 30.0)
                .unwrap();
        }
        live.complete_segment(segment).unwrap();
        assert_eq!(live.effective(&first).unwrap().appearance, 1.0);
        assert_eq!(live.effective(&second).unwrap().appearance, 1.0);
    }

    #[test]
    fn fade_in_accepts_detached_family_members_but_rejects_reachable_targets() {
        for nested_family in [false, true] {
            let scene = Scene::new();
            let shape = scene.circle(0.5).unwrap();
            let family = scene.family(&[(&shape).into()]).unwrap();
            let mut session = scene.execution_session().unwrap();
            let mut live = scene.live(&mut session);
            let outer = live
                .family(&[MobjectFamilyMember::Family(&family)])
                .unwrap();
            let segment = if nested_family {
                live.declare_and_activate_family_fade(
                    &family,
                    SemanticFadeDirection::In,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .introducer(true),
                )
            } else {
                live.declare_and_activate_fade(
                    &shape,
                    SemanticFadeDirection::In,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
            }
            .unwrap();
            live.advance_segment_to(segment, segment.end_time())
                .unwrap();
            live.complete_segment(segment).unwrap();
            assert_eq!(live.effective(&shape).unwrap().appearance, 1.0);
            assert!(!scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(scene.root())
                .unwrap()
                .contains(&outer.node_id()));
            let before = live.session.publication_context();
            let rejected = if nested_family {
                live.declare_and_activate_family_fade(
                    &family,
                    SemanticFadeDirection::In,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear)
                        .introducer(true),
                )
            } else {
                live.declare_and_activate_fade(
                    &shape,
                    SemanticFadeDirection::In,
                    AnimationOptions::new()
                        .run_time(1.0)
                        .rate_func(RateFunction::Linear),
                )
            };
            assert!(rejected.is_err());
            assert_eq!(live.session.publication_context(), before);
        }
    }

    #[test]
    fn single_leaf_fade_enters_exits_and_readds_the_same_handle_locally() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        let fading = scene.circle(1.0).unwrap();
        scene.add(&anchor).unwrap();
        let fading_node = fading.node_id();
        let authored_before = fading.state().unwrap();
        let mut session = scene.execution_session().unwrap();
        let anchor_slot = session.execution_slot_for_frame_index(0).unwrap();
        session.take_frame_changes();

        let options = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        let fade_in = {
            let mut live = scene.live(&mut session);
            assert!(!live.contains(&fading).unwrap());
            let segment = live
                .declare_and_activate_fade(&fading, SemanticFadeDirection::In, options)
                .unwrap();
            assert!(live.contains(&fading).unwrap());
            assert_eq!(live.effective(&fading).unwrap().appearance, 0.0);
            let publication = live.session.last_structural_publication_stats();
            assert_eq!(publication.entered_objects, 1);
            assert_eq!(publication.exited_objects, 0);
            assert_eq!(publication.preparation.object_states_lowered, 1);
            assert_eq!(live.session.take_frame_changes().object_indices().len(), 1);
            segment
        };
        assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));

        {
            let mut live = scene.live(&mut session);
            live.advance_segment_to(fade_in, 0.5).unwrap();
            assert_eq!(live.effective(&fading).unwrap().appearance, 0.5);
            live.advance_segment_to(fade_in, fade_in.end_time())
                .unwrap();
            live.complete_segment(fade_in).unwrap();
            assert_eq!(live.effective(&fading).unwrap().appearance, 1.0);
            assert_eq!(live.authored(&fading).unwrap(), authored_before);
        }

        let fade_out = {
            let mut live = scene.live(&mut session);
            live.declare_and_activate_fade_with_endpoint(
                &fading,
                SemanticFadeDirection::Out,
                FadeEndpoint::new(
                    0.5,
                    FadeTranslation::Shift(SemanticVec3::new(2.0, 0.0, 0.0)),
                ),
                options,
            )
            .unwrap()
        };
        {
            let mut live = scene.live(&mut session);
            live.advance_segment_to(fade_out, 1.5).unwrap();
            assert_eq!(live.effective(&fading).unwrap().appearance, 0.5);
            assert_eq!(
                live.effective(&fading).unwrap().transform.translation.x,
                1.0
            );
            assert_eq!(live.effective(&fading).unwrap().transform.scale.x, 0.75);
            live.advance_segment_to(fade_out, fade_out.end_time())
                .unwrap();
            assert!(live.contains(&fading).unwrap());
            live.complete_segment(fade_out).unwrap();
            assert!(!live.contains(&fading).unwrap());
            assert!(live.effective(&fading).is_err());
            let publication = live.session.last_structural_publication_stats();
            assert_eq!(publication.entered_objects, 0);
            assert_eq!(publication.exited_objects, 1);

            live.add(&fading).unwrap();
            assert!(live.contains(&fading).unwrap());
            assert_eq!(live.effective(&fading).unwrap().appearance, 1.0);
            assert_eq!(fading.node_id(), fading_node);
            assert_eq!(live.authored(&fading).unwrap(), authored_before);
        }
        assert_eq!(session.execution_slot_for_frame_index(0), Some(anchor_slot));
    }

    #[test]
    fn fade_target_and_option_failures_leave_membership_and_publication_unchanged() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        let fading = scene.circle(1.0).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        let mut live = scene.live(&mut session);
        assert!(live
            .declare_and_activate_fade(
                &fading,
                SemanticFadeDirection::Out,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .is_err());
        assert!(live
            .declare_and_activate_fade_with_endpoint(
                &fading,
                SemanticFadeDirection::In,
                FadeEndpoint::new(f64::NAN, FadeTranslation::Shift(SemanticVec3::ZERO)),
                AnimationOptions::new().run_time(1.0),
            )
            .is_err());
        assert!(live
            .declare_and_activate_fade(
                &fading,
                SemanticFadeDirection::In,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.5),
            )
            .is_err());
        assert!(!live.contains(&fading).unwrap());
        assert_eq!(live.session.publication_context(), before);
        assert!(live.session.take_frame_changes().is_empty());
        assert!(live.session.execution_object_id(fading.node_id()).is_none());
    }
}

#[cfg(test)]
mod recursive_composition_tests {
    use super::*;
    use crate::Scene;
    use noon_core::RateFunction;

    fn linear(duration: f64) -> AnimationOptions {
        AnimationOptions::new()
            .run_time(duration)
            .rate_func(RateFunction::Linear)
    }

    #[test]
    fn nested_add_and_wait_publish_one_membership_batch_and_one_segment() {
        let scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = first.integration_store().borrow().len();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            options: AnimationOptions::new().rate_func(RateFunction::Smooth),
            children: vec![
                AnimationCompositionRequest::Add {
                    target: &first,
                    options: linear(0.2),
                },
                AnimationCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Sequence,
                    options: AnimationOptions::new().rate_func(RateFunction::Linear),
                    children: vec![
                        AnimationCompositionRequest::Wait { duration: 0.2 },
                        AnimationCompositionRequest::Add {
                            target: &second,
                            options: linear(0.2),
                        },
                    ],
                },
            ],
        };
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();

        assert!((segment.duration() - 0.6).abs() < 1e-12);
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert_eq!(first.integration_store().borrow().len(), before_nodes + 5);
        assert!(live.contains(&first).unwrap());
        assert!(live.contains(&second).unwrap());
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();

        let second_execution = live
            .session
            .execution_object_id(second.node_id())
            .expect("admitted leaf keeps one execution identity");
        let early = live.session.seek(segment.start_time() + 0.1).unwrap();
        let second_index = early
            .objects
            .iter()
            .position(|object| object.id == second_execution)
            .expect("admitted leaf keeps one runtime slot");
        assert!(!early.is_present(second_index));
        assert!(live
            .session
            .seek(segment.end_time())
            .unwrap()
            .is_present(second_index));
    }

    #[test]
    fn repeated_detached_add_is_rejected_before_any_publication() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::Add {
                    target: &square,
                    options: linear(0.2),
                },
                AnimationCompositionRequest::Add {
                    target: &square,
                    options: linear(0.2),
                },
            ],
        };
        let result = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new());
        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::CreateTarget {
                    error: crate::ExecutionSessionCreateError::DuplicateTarget,
                    ..
                }
            ))
        ));
        assert_eq!(session.publication_context(), before);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
    }

    #[test]
    fn composed_fade_out_completion_detaches_and_reenters_the_same_handle() {
        let mut scene = Scene::new();
        let fading = scene.circle(1.0).unwrap();
        let companion = scene.square(1.0).unwrap();
        scene.add(&fading).unwrap();
        scene.add(&companion).unwrap();
        for radius in [0.5, 0.6, 0.7] {
            let retained = scene.circle(radius).unwrap();
            scene.add(&retained).unwrap();
        }
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let options = linear(0.2);
        let fading_id = session.execution_object_id(fading.node_id()).unwrap();
        let companion_id = session.execution_object_id(companion.node_id()).unwrap();
        let fading_row = session
            .frame()
            .objects
            .iter()
            .position(|object| object.id == fading_id)
            .unwrap();
        let companion_row = session
            .frame()
            .objects
            .iter()
            .position(|object| object.id == companion_id)
            .unwrap();
        let row_count = session.frame().objects.len();

        let fade_out = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![
                AnimationCompositionRequest::Fade {
                    target: &fading,
                    direction: SemanticFadeDirection::Out,
                    endpoint: FadeEndpoint::default(),
                    options,
                },
                AnimationCompositionRequest::Fade {
                    target: &companion,
                    direction: SemanticFadeDirection::Out,
                    endpoint: FadeEndpoint::default(),
                    options,
                },
                AnimationCompositionRequest::Wait { duration: 0.1 },
            ],
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
        };
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(&fade_out, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();

        assert!(!live.contains(&fading).unwrap());
        assert!(!live.contains(&companion).unwrap());
        assert!(fading
            .integration_store()
            .borrow()
            .node(fading.node_id())
            .unwrap()
            .parents()
            .is_empty());
        assert_eq!(
            live.session.execution_object_id(fading.node_id()),
            Some(fading_id)
        );
        assert_eq!(
            live.session.execution_object_id(companion.node_id()),
            Some(companion_id)
        );

        let fade_in = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![
                AnimationCompositionRequest::Fade {
                    target: &fading,
                    direction: SemanticFadeDirection::In,
                    endpoint: FadeEndpoint::default(),
                    options,
                },
                AnimationCompositionRequest::Fade {
                    target: &companion,
                    direction: SemanticFadeDirection::In,
                    endpoint: FadeEndpoint::default(),
                    options,
                },
            ],
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
        };
        let reentry = live
            .declare_and_activate_composition(&fade_in, AnimationOptions::new())
            .unwrap();
        assert!(live.contains(&fading).unwrap());
        assert!(live.contains(&companion).unwrap());
        assert_eq!(live.session.frame().objects.len(), row_count);
        assert_eq!(
            live.session
                .frame()
                .objects
                .iter()
                .position(|object| object.id == fading_id),
            Some(fading_row)
        );
        assert_eq!(
            live.session
                .frame()
                .objects
                .iter()
                .position(|object| object.id == companion_id),
            Some(companion_row)
        );
        live.advance_segment_to(reentry, reentry.end_time())
            .unwrap();
        live.complete_segment(reentry).unwrap();
    }

    #[test]
    fn mixed_scalar_object_composition_publishes_and_completes_once() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let square = scene.square(1.0).unwrap();
        scene.add(&circle).unwrap();
        scene.add(&square).unwrap();
        let tracker = scene.value_tracker(0.0).unwrap();
        let position = scene
            .position_from_tracker(
                &tracker,
                noon_core::SemanticVec3::new(1.0, 0.0, 0.0),
                noon_core::SemanticVec3::new(-2.0, 1.0, 0.0),
            )
            .unwrap();
        scene.bind_position(&circle, &position).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
            children: vec![
                AnimationCompositionRequest::Wait { duration: 0.5 },
                AnimationCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    options: AnimationOptions::new().rate_func(RateFunction::Smooth),
                    children: vec![
                        AnimationCompositionRequest::ValueTracker {
                            tracker: &tracker,
                            target: 4.0,
                            options: linear(2.0),
                        },
                        AnimationCompositionRequest::Rotate {
                            target: &square,
                            angle: std::f64::consts::PI,
                            options: linear(2.0),
                        },
                    ],
                },
            ],
        };
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();

        assert_eq!(segment.duration(), 2.5);
        assert!(segment.token().is_some());
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_signal_state(tracker.node_id())
                .unwrap()
                .scalar_timeline()
                .len(),
            1
        );

        live.advance_segment_to(segment, 0.25).unwrap();
        assert_eq!(
            live.session.effective_signal_value(tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(0.0))
        );
        live.advance_segment_to(segment, 1.5).unwrap();
        let forward_objects = live.session.frame().objects.clone();
        assert_eq!(
            live.session.effective_signal_value(tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(2.0))
        );
        live.session.seek(0.25).unwrap();
        live.session.seek(1.5).unwrap();
        assert_eq!(live.session.frame().objects, forward_objects);
        assert_eq!(
            live.session.effective_signal_value(tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(2.0))
        );

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        let before_completion = live.session.publication_context().scene_revision();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.session.publication_context().scene_revision(),
            before_completion.checked_next().unwrap()
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_signal_state(tracker.node_id())
                .unwrap()
                .scalar_timeline()
                .len(),
            2
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_input_scalar_value_at(tracker.node_id(), segment.end_time()),
            Ok(4.0)
        );
        assert!(
            (square.state().unwrap().transform.rotation_z - std::f64::consts::PI).abs() < 1e-12
        );

        live.set_value(&tracker, 3.0).unwrap();
        assert_eq!(
            live.session.effective_signal_value(tracker.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(3.0))
        );
    }

    #[test]
    fn invalid_mixed_sibling_rolls_back_tracker_scope_and_object_admission() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let tracker = ValueTracker::detached(Rc::clone(scene.integration_store()), 0.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let before_nodes = scene.integration_store().borrow().len();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::Add {
                    target: &square,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::ValueTracker {
                    tracker: &tracker,
                    target: f64::NAN,
                    options: linear(1.0),
                },
            ],
        };

        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert!(!scene
            .integration_store()
            .borrow()
            .has_semantic_signal_scope(tracker.node_id()));
        assert!(session.frame().objects.is_empty());
    }

    #[test]
    fn two_detached_trackers_enroll_in_one_mixed_publication() {
        let scene = Scene::new();
        let first = ValueTracker::detached(Rc::clone(scene.integration_store()), 1.0).unwrap();
        let second = ValueTracker::detached(Rc::clone(scene.integration_store()), -2.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::ValueTracker {
                    tracker: &first,
                    target: 3.0,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::ValueTracker {
                    tracker: &second,
                    target: 4.0,
                    options: linear(1.0),
                },
            ],
        };

        let segment = scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();

        assert_eq!(segment.duration(), 1.0);
        assert_eq!(
            session.publication_context().scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert!(scene
            .integration_store()
            .borrow()
            .has_semantic_signal_scope(first.node_id()));
        assert!(scene
            .integration_store()
            .borrow()
            .has_semantic_signal_scope(second.node_id()));
        assert_eq!(
            session.effective_signal_value(first.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(1.0))
        );
        assert_eq!(
            session.effective_signal_value(second.node_id()),
            Some(&noon_core::ReactiveValue::Scalar(-2.0))
        );
    }

    #[test]
    fn invalid_second_detached_tracker_rolls_back_both_enrollments() {
        let scene = Scene::new();
        let first = ValueTracker::detached(Rc::clone(scene.integration_store()), 1.0).unwrap();
        let second = ValueTracker::detached(Rc::clone(scene.integration_store()), -2.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::ValueTracker {
                    tracker: &first,
                    target: 3.0,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::ValueTracker {
                    tracker: &second,
                    target: f64::NAN,
                    options: linear(1.0),
                },
            ],
        };

        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert!(!scene
            .integration_store()
            .borrow()
            .has_semantic_signal_scope(first.node_id()));
        assert!(!scene
            .integration_store()
            .borrow()
            .has_semantic_signal_scope(second.node_id()));
        assert_eq!(session.effective_signal_value(first.node_id()), None);
        assert_eq!(session.effective_signal_value(second.node_id()), None);
    }

    #[test]
    fn ordered_family_transform_applies_lag_to_authoritative_member_order() {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let mut first_target = first.target_editor().unwrap();
        let mut second_target = second.target_editor().unwrap();
        first_target.set_translation(3.0, 0.0).unwrap();
        second_target.set_translation(6.0, 0.0).unwrap();
        let source = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let target = scene
            .family(&[(&first_target).into(), (&second_target).into()])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_family_transform_to(
                &source,
                &target,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.5),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 0.25)
            .unwrap();
        assert!(live.effective(&first).unwrap().transform.translation.x > 0.0);
        assert_eq!(
            live.effective(&second).unwrap().transform.translation.x,
            0.0
        );
    }

    #[test]
    fn live_family_arrange_uses_detached_authored_bounds_in_one_publication() {
        let scene = Scene::new();
        let first = scene.square(0.4).unwrap();
        let second = scene.circle(0.2).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let mut live = scene.live(&mut session);

        live.arrange_family(&family, 1.0, 0.0, 0.2, true).unwrap();
        let publication = live.session.publication_context();
        let first_center = first.center().unwrap();
        let second_center = second.center().unwrap();

        assert_ne!(publication, before);
        assert_eq!(
            publication.scene_revision(),
            before.scene_revision().checked_next().unwrap()
        );
        assert!((second_center.0 - first_center.0 - 0.6).abs() < 1e-6);
        assert!((first_center.0 + second_center.0).abs() < 1e-6);
    }

    #[test]
    fn live_family_arrange_centers_unique_detached_and_reachable_members() {
        for mounted in [false, true] {
            let mut scene = Scene::new();
            let first = scene.circle(0.2).unwrap();
            let mut second = scene.circle(0.2).unwrap();
            second.shift(2.0, 0.0).unwrap();
            let unrelated = scene.square(1.0).unwrap();
            let nested = scene.family(&[(&first).into(), (&second).into()]).unwrap();
            let outer = scene.family(&[(&first).into()]).unwrap();
            scene
                .integration_store()
                .borrow_mut()
                .add_member(outer.node_id(), nested.node_id())
                .unwrap();
            scene.add(&unrelated).unwrap();
            if mounted {
                scene.add(&first).unwrap();
                scene.add(&second).unwrap();
            }
            let mut session = scene.execution_session().unwrap();
            let mut live = scene.live(&mut session);
            let before = live.session.publication_context();
            let unrelated_before = live.effective(&unrelated).unwrap();

            assert!(live
                .arrange_family(&outer, 1.0, 0.0, f64::NAN, true)
                .is_err());
            assert_eq!(live.session.publication_context(), before);
            assert_eq!(first.center().unwrap(), (0.0, 0.0));
            assert_eq!(second.center().unwrap(), (2.0, 0.0));

            live.arrange_family(&outer, 1.0, 0.0, 0.2, true).unwrap();
            assert_eq!(
                live.session.publication_context().scene_revision(),
                before.scene_revision().checked_next().unwrap()
            );
            assert!((first.center().unwrap().0 + 1.0).abs() < 1e-6);
            assert!((second.center().unwrap().0 - 1.0).abs() < 1e-6);
            let unrelated_after = live.effective(&unrelated).unwrap();
            assert_eq!(unrelated_after.transform, unrelated_before.transform);
            assert_eq!(unrelated_after.style, unrelated_before.style);
            assert_eq!(unrelated_after.appearance, unrelated_before.appearance);
            if mounted {
                assert!(
                    (live.effective(&first).unwrap().transform.translation.x + 1.0).abs() < 1e-6
                );
                assert!(
                    (live.effective(&second).unwrap().transform.translation.x - 1.0).abs() < 1e-6
                );
            }
        }
    }

    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    #[test]
    fn family_fade_preserves_family_membership_and_ordered_lifecycle() {
        let scene = Scene::new();
        let label = scene.text(crate::Text::new("Fade")).unwrap();
        let shape = scene.circle(0.25).unwrap();
        let family = scene.family(&[(&label).into(), (&shape).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let fade_in = live
            .declare_and_activate_family_fade(
                &family,
                SemanticFadeDirection::In,
                linear(1.0).introducer(true),
            )
            .unwrap();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(scene.root())
                .unwrap(),
            [family.node_id()]
        );
        assert_eq!(live.effective(&label).unwrap().appearance, 0.0);
        assert_eq!(live.effective(&shape).unwrap().appearance, 0.0);
        live.advance_segment_to(fade_in, fade_in.end_time())
            .unwrap();
        live.complete_segment(fade_in).unwrap();

        let segment = live
            .declare_and_activate_family_fade(
                &family,
                SemanticFadeDirection::Out,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
                    .lag_ratio(0.5)
                    .remover(true),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
        assert!(
            live.effective(&label).unwrap().appearance < live.effective(&shape).unwrap().appearance
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();

        let store = scene.integration_store().borrow();
        assert!(store
            .semantic_family_members_checked(scene.root())
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .semantic_family_members_checked(family.node_id())
                .unwrap(),
            [label.node_id(), shape.node_id()]
        );
    }

    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    #[test]
    fn family_fade_and_disjoint_text_write_share_one_atomic_composition() {
        let mut scene = Scene::new();
        let fading_text = scene.text(crate::Text::new("old")).unwrap();
        let fading_shape = scene.square(0.5).unwrap();
        let family = scene
            .family(&[(&fading_text).into(), (&fading_shape).into()])
            .unwrap();
        scene
            .add_many(&[MobjectFamilyMember::Family(&family)])
            .unwrap();
        let written = scene.text(crate::Text::new("new")).unwrap();
        let mut session = scene.execution_session().unwrap();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
            children: vec![
                AnimationCompositionRequest::FamilyFade {
                    target: &family,
                    direction: SemanticFadeDirection::Out,
                    options: linear(1.0).remover(true),
                },
                AnimationCompositionRequest::TextWrite {
                    target: &written,
                    reverse_member_order: false,
                    options: linear(1.0).introducer(true),
                },
            ],
        };
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();

        let store = scene.integration_store().borrow();
        assert_eq!(
            store.semantic_family_members_checked(scene.root()).unwrap(),
            [written.node_id()]
        );
    }

    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    #[test]
    fn family_fade_rejects_overlapping_text_write_before_publication() {
        let mut scene = Scene::new();
        let label = scene.text(crate::Text::new("same")).unwrap();
        let family = scene.family(&[(&label).into()]).unwrap();
        scene
            .add_many(&[MobjectFamilyMember::Family(&family)])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::FamilyFade {
                    target: &family,
                    direction: SemanticFadeDirection::Out,
                    options: linear(1.0).remover(true),
                },
                AnimationCompositionRequest::TextWrite {
                    target: &label,
                    reverse_member_order: false,
                    options: linear(1.0).introducer(false).remover(false),
                },
            ],
        };

        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .semantic_family_members_checked(scene.root())
                .unwrap(),
            [family.node_id()]
        );
    }

    #[test]
    fn invalid_family_topology_rolls_back_before_declaration() {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let target = first.target_editor().unwrap();
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let source = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let nested = {
            let mut transaction = SemanticMutationTransaction::new();
            let inner = transaction.create_node(noon_core::SemanticNodeCreation::family());
            transaction.add_member(inner, target.node_id());
            let outer = transaction.create_node(noon_core::SemanticNodeCreation::family());
            transaction.add_member(outer, inner);
            let result = transaction
                .apply(&mut scene.integration_store().borrow_mut())
                .unwrap();
            MobjectFamily::from_node(
                Rc::clone(scene.integration_store()),
                result.resolve(outer).unwrap(),
            )
            .unwrap()
        };
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let before_nodes = scene.integration_store().borrow().len();

        assert!(scene
            .live(&mut session)
            .declare_and_activate_family_transform_to(&source, &nested, AnimationOptions::new(),)
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
    }

    #[test]
    fn indicate_restores_the_activation_effective_source() {
        let mut scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        scene.add(&square).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        live.scale(&square, 1.5, 0.75).unwrap();
        let source = live.effective(&square).unwrap();
        let segment = live
            .declare_and_activate_indicate(
                &square,
                IndicateOptions::default(),
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::ThereAndBack),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        let outward = live.effective(&square).unwrap();
        assert!((outward.transform.scale.x - source.transform.scale.x * 1.2).abs() < 1e-5);
        assert!((outward.transform.scale.y - source.transform.scale.y * 1.2).abs() < 1e-5);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        let restored = live.effective(&square).unwrap();
        assert_eq!(restored.transform, source.transform);
        assert_eq!(restored.style, source.style);
    }

    #[test]
    fn same_family_transform_then_indicate_is_rejected_atomically() {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let mut first_target = first.target_editor().unwrap();
        let mut second_target = second.target_editor().unwrap();
        first_target.set_translation(2.0, 0.0).unwrap();
        second_target.set_translation(4.0, 0.0).unwrap();
        let source = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let target = scene
            .family(&[(&first_target).into(), (&second_target).into()])
            .unwrap();
        let mut session = scene.execution_session().unwrap();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
            children: vec![
                AnimationCompositionRequest::FamilyTransformTo {
                    source: &source,
                    target_state: &target,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::FamilyIndicate {
                    target: &source,
                    indication: IndicateOptions::default(),
                    options: AnimationOptions::new()
                        .run_time(2.0)
                        .rate_func(RateFunction::ThereAndBack),
                },
            ],
        };
        let mut live = scene.live(&mut session);
        let before = live.session.publication_context();
        assert!(live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .is_err());
        assert_eq!(live.session.publication_context(), before);
        assert_eq!(live.effective(&first).unwrap().transform.translation.x, 0.0);
        assert_eq!(
            live.effective(&second).unwrap().transform.translation.x,
            0.0
        );
    }

    #[test]
    fn family_indicate_rejects_an_earlier_center_affecting_rotation() {
        let mut scene = Scene::new();
        let first = scene.rectangle(2.0, 1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Sequence,
            options: AnimationOptions::new(),
            children: vec![
                AnimationCompositionRequest::Rotate {
                    target: &first,
                    angle: std::f64::consts::FRAC_PI_2,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::FamilyIndicate {
                    target: &family,
                    indication: IndicateOptions::default(),
                    options: AnimationOptions::new().run_time(2.0),
                },
            ],
        };

        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .is_err());
        assert_eq!(session.publication_context(), before);
    }

    #[test]
    fn family_indicate_scales_members_about_the_shared_family_center() {
        let mut scene = Scene::new();
        let mut left = scene.square(1.0).unwrap();
        let mut right = scene.square(1.0).unwrap();
        left.set_translation(-2.0, 0.0).unwrap();
        right.set_translation(2.0, 0.0).unwrap();
        scene.add(&left).unwrap();
        scene.add(&right).unwrap();
        let family = scene.family(&[(&left).into(), (&right).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_family_indicate(
                &family,
                IndicateOptions::new(2.0, noon_core::YELLOW),
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        assert_eq!(live.effective(&left).unwrap().transform.translation.x, -4.0);
        assert_eq!(live.effective(&right).unwrap().transform.translation.x, 4.0);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(live.effective(&left).unwrap().transform.translation.x, -2.0);
        assert_eq!(live.effective(&right).unwrap().transform.translation.x, 2.0);
    }

    #[test]
    fn draw_border_then_fill_holds_the_explicit_outline_through_the_reveal_phase() {
        let scene = Scene::new();
        let mut square = scene.square(1.0).unwrap();
        square
            .set_fill(
                f64::from(Color::ORANGE.red),
                f64::from(Color::ORANGE.green),
                f64::from(Color::ORANGE.blue),
                1.0,
            )
            .unwrap();
        square
            .set_stroke_color(
                f64::from(Color::BLUE.red),
                f64::from(Color::BLUE.green),
                f64::from(Color::BLUE.blue),
                1.0,
            )
            .unwrap();
        square.set_stroke_width(0.06).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_draw_border_then_fill(
                &square,
                DrawBorderThenFillOptions::new(0.04, Some(Color::YELLOW)),
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear)
                    .introducer(true),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
        let outline = live.effective(&square).unwrap().style;
        assert_eq!(outline.fill.map(|fill| fill.alpha), Some(0.0));
        assert_eq!(outline.stroke, Some(Color::YELLOW));
        assert_eq!(outline.stroke_width, 0.04);

        live.advance_segment_to(segment, segment.start_time() + 1.5)
            .unwrap();
        let filling = live.effective(&square).unwrap().style;
        assert_eq!(filling.fill.map(|fill| fill.alpha), Some(0.5));
        let stroke = filling.stroke.expect("fill phase retains a stroke");
        assert!((stroke.red - (Color::YELLOW.red + Color::BLUE.red) * 0.5).abs() < 1e-6);
        assert!((stroke.green - (Color::YELLOW.green + Color::BLUE.green) * 0.5).abs() < 1e-6);
        assert!((stroke.blue - (Color::YELLOW.blue + Color::BLUE.blue) * 0.5).abs() < 1e-6);
        assert!((filling.stroke_width - 0.05).abs() < 1e-6);

        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        let restored = live.effective(&square).unwrap().style;
        assert_eq!(restored.fill, Some(Color::ORANGE));
        assert_eq!(restored.stroke, Some(Color::BLUE));
        assert_eq!(restored.stroke_width, 0.06);
    }

    #[test]
    fn increasing_subsets_uses_absolute_floor_thresholds_and_retains_members() {
        let scene = Scene::new();
        let mut first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        first.set_fill_opacity(0.35).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        family.prepare_subset_display().unwrap();
        assert_eq!(first.state().unwrap().style.fill_opacity, 0.0);
        assert_eq!(second.state().unwrap().style.fill_opacity, 0.0);

        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_family_subset_display(
                &family,
                SubsetDisplayMode::IncreasingFloor,
                AnimationOptions::new()
                    .run_time(2.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.start_time())
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        live.advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            1.0
        );
    }

    #[test]
    fn one_by_one_default_smooth_keeps_the_prior_member_at_the_exact_boundary() {
        let scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        family.prepare_subset_display().unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_family_subset_display(
                &family,
                SubsetDisplayMode::OneByOneCeil,
                AnimationOptions::new().run_time(2.0),
            )
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        live.advance_segment_to(segment, segment.start_time() + 1.001)
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(first.state().unwrap().style.fill_opacity, 0.0);
        assert_eq!(second.state().unwrap().style.fill_opacity, 1.0);
    }

    #[test]
    fn one_by_one_preserves_the_strict_boundary_through_nested_smooth_maps() {
        let scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        family.prepare_subset_display().unwrap();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children: vec![AnimationCompositionRequest::FamilySubsetDisplay {
                target: &family,
                mode: SubsetDisplayMode::OneByOneCeil,
                options: AnimationOptions::new(),
            }],
            options: AnimationOptions::new()
                .run_time(2.0)
                .rate_func(RateFunction::Smooth),
        };
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let segment = live
            .declare_and_activate_composition(&request, AnimationOptions::new())
            .unwrap();

        live.advance_segment_to(segment, segment.start_time() + 1.0)
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        live.advance_segment_to(segment, segment.start_time() + 1.001)
            .unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            0.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            1.0
        );
    }

    #[test]
    fn three_member_one_by_one_keeps_prior_member_at_fractional_boundaries() {
        let scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        let third = scene.square(1.0).unwrap();
        let family = scene
            .family(&[(&first).into(), (&second).into(), (&third).into()])
            .unwrap();
        family.prepare_subset_display().unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);

        let wait = live.wait_segment(3.0).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        live.complete_segment(wait).unwrap();
        let segment = live
            .declare_and_activate_family_subset_display(
                &family,
                SubsetDisplayMode::OneByOneCeil,
                AnimationOptions::new()
                    .run_time(3.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();
        assert_eq!(segment.start_time(), 3.0);

        live.advance_segment_to(segment, 4.0).unwrap();
        assert_eq!(
            [
                live.effective(&first).unwrap().style.fill.unwrap().alpha,
                live.effective(&second).unwrap().style.fill.unwrap().alpha,
                live.effective(&third).unwrap().style.fill.unwrap().alpha,
            ],
            [1.0, 0.0, 0.0]
        );
        live.advance_segment_to(segment, 4.001).unwrap();
        assert_eq!(
            [
                live.effective(&first).unwrap().style.fill.unwrap().alpha,
                live.effective(&second).unwrap().style.fill.unwrap().alpha,
                live.effective(&third).unwrap().style.fill.unwrap().alpha,
            ],
            [0.0, 1.0, 0.0]
        );
        live.advance_segment_to(segment, 5.0).unwrap();
        assert_eq!(
            [
                live.effective(&first).unwrap().style.fill.unwrap().alpha,
                live.effective(&second).unwrap().style.fill.unwrap().alpha,
                live.effective(&third).unwrap().style.fill.unwrap().alpha,
            ],
            [0.0, 1.0, 0.0]
        );
        live.advance_segment_to(segment, 5.001).unwrap();
        assert_eq!(
            [
                live.effective(&first).unwrap().style.fill.unwrap().alpha,
                live.effective(&second).unwrap().style.fill.unwrap().alpha,
                live.effective(&third).unwrap().style.fill.unwrap().alpha,
            ],
            [0.0, 0.0, 1.0]
        );
    }

    #[test]
    fn live_subset_preparation_is_one_atomic_style_publication() {
        let mut scene = Scene::new();
        let first = scene.square(1.0).unwrap();
        let second = scene.square(1.0).unwrap();
        scene.add(&first).unwrap();
        scene.add(&second).unwrap();
        let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        let result = scene
            .live(&mut session)
            .prepare_family_subset_display(&family)
            .unwrap();
        assert_eq!(result.impacts().len(), 2);
        assert_eq!(
            session.publication_context().scene_revision().get(),
            before.scene_revision().get() + 1
        );
        assert_eq!(session.frame().objects[0].style.fill.unwrap().alpha, 0.0);
        assert_eq!(session.frame().objects[1].style.fill.unwrap().alpha, 0.0);
    }

    #[test]
    fn live_created_detached_family_prepares_and_enters_after_wait() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.25).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let mut live = scene.live(&mut session);

        let wait = live.wait_segment(1.0).unwrap();
        live.advance_segment_to(wait, wait.end_time()).unwrap();
        let first = live
            .create_manim_geometry(crate::ManimGeometryOptions::square(0.5).unwrap())
            .unwrap();
        let second = live
            .create_manim_geometry(crate::ManimGeometryOptions::circle(0.25).unwrap())
            .unwrap();
        let family = live
            .family(&[
                MobjectFamilyMember::Mobject(&first),
                MobjectFamilyMember::Mobject(&second),
            ])
            .unwrap();

        live.prepare_family_subset_display(&family).unwrap();
        assert_eq!(live.authored(&first).unwrap().style.fill_opacity, 0.0);
        assert_eq!(live.authored(&second).unwrap().style.fill_opacity, 0.0);
        let segment = live
            .declare_and_activate_family_subset_display(
                &family,
                SubsetDisplayMode::IncreasingFloor,
                AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(RateFunction::Linear),
            )
            .unwrap();

        assert_eq!(segment.start_time(), wait.end_time());
        assert!(live.contains(&first).unwrap());
        assert!(live.contains(&second).unwrap());
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();
        assert_eq!(
            live.effective(&first).unwrap().style.fill.unwrap().alpha,
            1.0
        );
        assert_eq!(
            live.effective(&second).unwrap().style.fill.unwrap().alpha,
            1.0
        );
    }

    #[test]
    fn subset_preparation_rejects_nested_families_without_partial_style_changes() {
        let scene = Scene::new();
        let mut first = scene.square(1.0).unwrap();
        first.set_fill_opacity(1.0).unwrap();
        let nested_member = scene.square(1.0).unwrap();
        let nested = scene.family(&[(&nested_member).into()]).unwrap();
        let mut transaction = SemanticMutationTransaction::new();
        let outer = transaction.create_node(noon_core::SemanticNodeCreation::family());
        transaction.add_member(outer, first.node_id());
        transaction.add_member(outer, nested.node_id());
        let result = transaction
            .apply(&mut scene.integration_store().borrow_mut())
            .unwrap();
        let outer = MobjectFamily::from_node(
            Rc::clone(scene.integration_store()),
            result.resolve(outer).unwrap(),
        )
        .unwrap();

        assert!(outer.prepare_subset_display().is_err());
        assert_eq!(first.state().unwrap().style.fill_opacity, 1.0);

        let mut session = scene.execution_session().unwrap();
        let before = session.publication_context();
        assert!(scene
            .live(&mut session)
            .declare_and_activate_family_subset_display(
                &outer,
                SubsetDisplayMode::IncreasingFloor,
                AnimationOptions::new(),
            )
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(scene.root())
            .unwrap()
            .is_empty());
    }

    #[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
    #[test]
    fn family_text_write_admits_and_unwrite_removes_one_family_root_atomically() {
        let scene = Scene::new();
        let left = scene.text(crate::Text::new("A")).unwrap();
        let right = scene.text(crate::Text::new("BCDE")).unwrap();
        let family = scene.family(&[(&left).into(), (&right).into()]).unwrap();
        let mut session = scene.execution_session().unwrap();

        let conflicting = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
            children: vec![
                AnimationCompositionRequest::FamilyTextWrite {
                    target: &family,
                    reverse_member_order: false,
                    options: linear(1.0),
                },
                AnimationCompositionRequest::TextWrite {
                    target: &left,
                    reverse_member_order: false,
                    options: linear(1.0),
                },
            ],
        };
        let before = session.publication_context();
        assert!(scene
            .live(&mut session)
            .declare_and_activate_composition(&conflicting, AnimationOptions::new())
            .is_err());
        assert_eq!(session.publication_context(), before);
        assert!(scene
            .integration_store()
            .borrow()
            .node(family.node_id())
            .unwrap()
            .parents()
            .is_empty());

        let mut live = scene.live(&mut session);
        let write = live
            .declare_and_activate_family_text_write(&family, false, linear(1.0).introducer(true))
            .unwrap();
        live.advance_segment_to(write, write.end_time()).unwrap();
        live.complete_segment(write).unwrap();
        for member in [&left, &right] {
            assert!(noon_core::semantic_scene_root_contains(
                &scene.integration_store().borrow(),
                scene.root(),
                member.node_id(),
            )
            .unwrap());
        }

        let unwrite = live
            .declare_and_activate_family_text_write(
                &family,
                true,
                linear(1.0).introducer(false).remover(true),
            )
            .unwrap();
        live.advance_segment_to(unwrite, unwrite.end_time())
            .unwrap();
        live.complete_segment(unwrite).unwrap();
        for member in [&left, &right] {
            assert!(!noon_core::semantic_scene_root_contains(
                &scene.integration_store().borrow(),
                scene.root(),
                member.node_id(),
            )
            .unwrap());
        }
        let store = family.integration_store().borrow();
        assert!(store.node(family.node_id()).is_some());
        assert_eq!(
            store
                .semantic_family_members_checked(family.node_id())
                .unwrap(),
            [left.node_id(), right.node_id()]
        );
    }
}
