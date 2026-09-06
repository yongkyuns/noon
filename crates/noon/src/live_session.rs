//! Borrowed live access to one already-published execution session.
//!
//! This facade owns neither semantic nor runtime state.  It only coordinates a
//! transaction with the session that already lowered the same semantic store.
//! Membership and property publication use the same prepared semantic transaction.
//! Existing affine declarations use session-local segments, whose endpoint
//! reconciliation remains owned by `ExecutionSession::complete_segment`.

use crate::{
    semantic_mobject::authoring_render_f64,
    semantic_mobject::{
        edit_color, edit_disable_fill, edit_disable_stroke, edit_fill, edit_fill_color,
        edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke, edit_stroke_color,
        edit_stroke_opacity,
    },
    DeclaredAnimation, EffectiveSemanticObject, ExecutionSegment, ExecutionSegmentAdvanceError,
    ExecutionSegmentCompletionError, ExecutionSegmentError, ExecutionSegmentState,
    ExecutionSession, ExecutionSessionAnimationError, ExecutionSessionPublicationError, Mobject,
    ValueTracker,
};
use noon_core::{
    AnimationOptions, Bounds2D64, Color, PublicationContext, SemanticAffineLifecycleDirection,
    SemanticAffineLifecycleEndpoint, SemanticAnimationCompositionKind, SemanticFadeDirection,
    SemanticMutationTransaction, SemanticMutationTransactionResult, SemanticNodeId,
    SemanticObjectProperty, SemanticObjectState, SemanticSignalValue, SemanticStore, SemanticStyle,
    SemanticVec3, Style, Transform2D,
};
use std::{cell::RefCell, rc::Rc};

/// An owned observation of one effective runtime object at one publication.
///
/// The clone makes the observation safe to retain after the next live mutation;
/// it is an observation, not another runtime authority.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectiveMobjectState {
    pub transform: Transform2D,
    pub style: Style,
    pub appearance: f32,
    pub publication: PublicationContext,
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

/// Reconstruct the supported authored target style directly from one effective
/// runtime row. Runtime colors already contain the evaluated paint opacity, so
/// the detached target stores them as solid paints with unit paint opacity and
/// preserves the row's object opacity separately. Resource paints are not
/// represented by `Style` and must remain explicitly unavailable here.
fn target_style_from_effective(
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
        return Err(LiveSessionError::Mobject(
            "target editor cannot capture a runtime style backed by a paint resource".into(),
        ));
    }
    Ok(SemanticStyle {
        fill: effective.fill.map(noon_core::SemanticPaint::Solid),
        fill_opacity: 1.0,
        stroke: effective.stroke.map(noon_core::SemanticPaint::Solid),
        stroke_opacity: 1.0,
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
        object_opacity: f64::from(effective.opacity),
    })
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
    options: AnimationOptions,
}

/// One typed leaf in an atomic live animation composition.
#[derive(Clone, Copy)]
pub enum AnimationCompositionRequest<'a> {
    /// Transform a source toward an authored target using the requested interpolation.
    TransformTo(TransformToRequest<'a>),
    /// Rotate a centered 2D leaf along an angular path.
    Rotate {
        /// The authored semantic object affected by this leaf.
        target: &'a Mobject,
        /// Signed 2D rotation in radians.
        angle: f64,
        /// Leaf-local authored timing options.
        options: AnimationOptions,
    },
}

impl<'a> TransformToRequest<'a> {
    pub const fn new(
        source: &'a Mobject,
        target_state: &'a Mobject,
        options: AnimationOptions,
    ) -> Self {
        Self {
            source,
            target_state,
            interpolation: noon_core::SemanticTransformInterpolation::Affine,
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
            options,
        }
    }
}

/// Errors while a semantic handle is used through a live execution session.
#[derive(Debug)]
pub enum LiveSessionError {
    ForeignMobjectStore,
    Mobject(String),
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
            Self::Mobject(error) => error.fmt(formatter),
            Self::Animation(error) => error.fmt(formatter),
            Self::Activation(error) => error.fmt(formatter),
            Self::Segment(error) => error.fmt(formatter),
            Self::Advance(error) => error.fmt(formatter),
            Self::Completion(error) => error.fmt(formatter),
            Self::Publication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LiveSessionError {}

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
            .map_err(LiveSessionError::Animation)?;
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
    /// runtime. Unsupported content, ordering, and structural work fails before
    /// either layer commits.
    pub fn apply(
        &mut self,
        transaction: SemanticMutationTransaction,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let mut store = self.store.borrow_mut();
        self.session
            .apply_semantic_transaction(&mut store, transaction)
            .map_err(Into::into)
    }

    /// Add an existing detached object to this live scene root.
    pub fn add(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_member(self.root, mobject.node_id());
        self.apply(transaction)
    }

    /// Remove an existing object from this live scene root without deleting identity.
    pub fn remove(
        &mut self,
        mobject: &Mobject,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.remove_member(self.root, mobject.node_id());
        self.apply(transaction)
    }

    /// Check whether a handle is currently a direct member of this live scene root.
    pub fn contains(&self, mobject: &Mobject) -> Result<bool, LiveSessionError> {
        self.require_mobject(mobject)?;
        self.store
            .borrow()
            .is_direct_member(self.root, mobject.node_id())
            .map_err(|error| LiveSessionError::Mobject(error.to_string()))
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
        let content = source.state().map_err(LiveSessionError::Mobject)?.content;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.replace_content(target.node_id(), content);
        self.apply(transaction)
    }

    /// Inspect authored/base state explicitly, separate from [`Self::effective`].
    pub fn authored(&self, mobject: &Mobject) -> Result<SemanticObjectState, LiveSessionError> {
        self.require_mobject(mobject)?;
        mobject.state().map_err(LiveSessionError::Mobject)
    }

    /// Create a detached, session-coherent target copy for subsequent live authoring.
    ///
    /// Detached target edits advance the same semantic/runtime publication context but produce
    /// no execution object or frame work. This keeps later atomic animation declaration valid
    /// without resetting or relowering the active runtime.
    pub fn target_editor(&mut self, source: &Mobject) -> Result<Mobject, LiveSessionError> {
        self.require_mobject(source)?;
        if self.session.pending_callback_token().is_some() {
            return Err(LiveSessionError::Mobject(
                "cannot create a target while a required callback phase is pending".into(),
            ));
        }
        if self.session.callback_termination().is_some() {
            return Err(LiveSessionError::Mobject(
                "cannot create a target from a terminated callback session".into(),
            ));
        }

        // A target created after bootstrap must start from the coherent effective row,
        // rather than the authored base that an active driver or callback may have
        // superseded. Immutable content remains authored. This subset intentionally
        // rejects render-content and appearance overrides because SemanticObjectState
        // has no exact authored representation for them.
        let mut state = source.state().map_err(LiveSessionError::Mobject)?;
        if !state.signal_bindings().is_empty() {
            return Err(LiveSessionError::Mobject(
                "target editor cannot capture a reactive binding into a detached target".into(),
            ));
        }
        let store = self.store.borrow();
        let observed = self
            .session
            .effective_semantic_object(&store, source.node_id())?;
        if !observed.authored_content_layout_applicable() {
            return Err(LiveSessionError::Mobject(
                "target editor requires effective authored content without reveal or morph overrides"
                    .into(),
            ));
        }
        if observed.object.appearance != 1.0 {
            return Err(LiveSessionError::Mobject(
                "target editor cannot represent a non-unit effective appearance".into(),
            ));
        }
        state.transform.translation.x = f64::from(observed.object.transform.translation.x);
        state.transform.translation.y = f64::from(observed.object.transform.translation.y);
        state.transform.scale.x = f64::from(observed.object.transform.scale.x);
        state.transform.scale.y = f64::from(observed.object.transform.scale.y);
        state.transform.rotation_z = f64::from(observed.object.transform.rotation);
        state.style = target_style_from_effective(&state.style, observed.object.style)?;
        drop(store);

        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(noon_core::SemanticNodeCreation::object(state));
        let result = self.apply(transaction)?;
        let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("one prepared target copy has one exact semantic impact")
        };
        Mobject::from_node(Rc::clone(self.store), *node).map_err(LiveSessionError::Mobject)
    }

    /// Publish one fully validated detached Manim primitive through this session.
    ///
    /// The new identity has no root membership, execution slot, or frame work
    /// until [`Self::add`] admits it.
    pub fn create_manim_primitive(
        &mut self,
        options: crate::ManimPrimitiveOptions,
    ) -> Result<Mobject, LiveSessionError> {
        self.create_detached_mobject(options.into_state())
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
        Mobject::from_node(Rc::clone(self.store), *node).map_err(LiveSessionError::Mobject)
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
            return Err(LiveSessionError::Mobject(
                "effective layout queries currently support affine and style drivers only".into(),
            ));
        }
        let transform = observed.object.transform;
        let publication = observed.publication;
        drop(store);
        self.layout_at_transform(mobject, transform, publication)
    }

    fn layout_at_transform(
        &self,
        mobject: &Mobject,
        transform: Transform2D,
        publication: PublicationContext,
    ) -> Result<EffectiveMobjectLayout, LiveSessionError> {
        let bounds = mobject
            .layout_bounds_at(transform)
            .map_err(LiveSessionError::Mobject)?;
        let (center, width, height) = if let Some(Bounds2D64 {
            min_x,
            min_y,
            max_x,
            max_y,
        }) = bounds
        {
            (
                ((min_x + max_x) * 0.5, (min_y + max_y) * 0.5),
                max_x - min_x,
                max_y - min_y,
            )
        } else {
            (
                (
                    f64::from(transform.translation.x),
                    f64::from(transform.translation.y),
                ),
                0.0,
                0.0,
            )
        };
        Ok(EffectiveMobjectLayout {
            center,
            width,
            height,
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
        tracker
            .require_store(self.store)
            .map_err(LiveSessionError::Animation)?;
        if !self
            .store
            .borrow()
            .is_semantic_signal_scoped(self.root, tracker.node_id())
        {
            return Err(LiveSessionError::Animation(
                "ValueTracker is not associated with this Scene".into(),
            ));
        }
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_value_tracker(
                &mut store,
                tracker.node_id(),
                target,
                duration,
                rate_func,
            )
            .map_err(Into::into)
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
            .map_err(LiveSessionError::Animation)?;
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
        self.require_mobject(target)?;
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_fade(&mut store, self.root, target.node_id(), direction, options)
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
        let endpoint = match endpoint {
            AffineLifecycleEndpoint::Point {
                x,
                y,
                rotation_offset,
                point_color,
            } => SemanticAffineLifecycleEndpoint {
                point: SemanticVec3::new(x, y, 0.0),
                rotation_offset,
                point_color,
            },
            AffineLifecycleEndpoint::EffectiveCenter => {
                if direction != AffineLifecycleDirection::RemoveTo {
                    return Err(LiveSessionError::Mobject(
                        "effective-center lifecycle endpoints require a live removal target".into(),
                    ));
                }
                let center = if self.contains(target)? {
                    self.effective_layout(target)?.center
                } else {
                    target.center().map_err(LiveSessionError::Mobject)?
                };
                SemanticAffineLifecycleEndpoint {
                    point: SemanticVec3::new(center.0, center.1, 0.0),
                    rotation_offset: 0.0,
                    point_color: None,
                }
            }
        };
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

    /// Atomically admit one detached leaf, reverse Reveal, and remove it at completion.
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

    /// Atomically author and activate one flat Parallel or Sequence of TransformTo leaves.
    ///
    /// All handles are checked before the semantic transaction is built. Rust snapshots the target
    /// states into that transaction, then completes schedule lowering, effective capture, and
    /// runtime preflight before any target, leaf, or root receives permanent identity.
    pub fn declare_and_activate_transform_composition(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: &[TransformToRequest<'_>],
        composition_options: AnimationOptions,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        for child in children {
            self.require_mobject(child.source)?;
            self.require_mobject(child.target_state)?;
        }
        let children = children
            .iter()
            .map(
                |child| crate::execution_session::SemanticCompositionRequest::TransformTo {
                    source: child.source.node_id(),
                    target_state: child.target_state.node_id(),
                    interpolation: child.interpolation,
                    options: child.options,
                },
            )
            .collect::<Vec<_>>();
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_mixed_composition(
                &mut store,
                kind,
                &children,
                composition_options,
                play_options,
                None,
            )
            .map_err(Into::into)
    }

    /// Atomically admit detached leaves and activate mixed point-transform/angular-path leaves.
    pub fn declare_and_activate_animation_composition(
        &mut self,
        kind: SemanticAnimationCompositionKind,
        children: &[AnimationCompositionRequest<'_>],
        composition_options: AnimationOptions,
        play_options: AnimationOptions,
    ) -> Result<ExecutionSegment, LiveSessionError> {
        for child in children {
            match child {
                AnimationCompositionRequest::TransformTo(child) => {
                    self.require_mobject(child.source)?;
                    self.require_mobject(child.target_state)?;
                }
                AnimationCompositionRequest::Rotate { target, .. } => {
                    self.require_mobject(target)?;
                }
            }
        }
        let children = children
            .iter()
            .map(|child| match child {
                AnimationCompositionRequest::TransformTo(child) => {
                    crate::execution_session::SemanticCompositionRequest::TransformTo {
                        source: child.source.node_id(),
                        target_state: child.target_state.node_id(),
                        interpolation: child.interpolation,
                        options: child.options,
                    }
                }
                AnimationCompositionRequest::Rotate {
                    target,
                    angle,
                    options,
                } => crate::execution_session::SemanticCompositionRequest::Rotate {
                    target: target.node_id(),
                    angle: *angle,
                    options: *options,
                },
            })
            .collect::<Vec<_>>();
        let mut store = self.store.borrow_mut();
        self.session
            .declare_and_activate_mixed_composition(
                &mut store,
                kind,
                &children,
                composition_options,
                play_options,
                Some(self.root),
            )
            .map_err(Into::into)
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

    /// Move an object's effective layout center to one point through a single
    /// shared translation publication. Layout evaluation is bounded to this
    /// object; the caller never reconstructs geometry or an affine offset.
    pub fn move_to_point(
        &mut self,
        mobject: &Mobject,
        x: f64,
        y: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let x = authoring_render_f64("move_to.x", x).map_err(LiveSessionError::Mobject)?;
        let y = authoring_render_f64("move_to.y", y).map_err(LiveSessionError::Mobject)?;
        let authored = self.authored(mobject)?;
        let authored_transform = Transform2D {
            translation: authored
                .transform
                .translation
                .lower_xy_f32()
                .map_err(|error| LiveSessionError::Mobject(error.to_string()))?,
            rotation: authoring_render_f64(
                "move_to authored rotation",
                authored.transform.rotation_z,
            )
            .map_err(LiveSessionError::Mobject)? as f32,
            scale: authored
                .transform
                .scale
                .lower_xy_f32()
                .map_err(|error| LiveSessionError::Mobject(error.to_string()))?,
        };
        let publication = self.session.publication_context();
        let store = self.store.borrow();
        match self
            .session
            .effective_semantic_object(&store, mobject.node_id())
        {
            Ok(observed) if !observed.authored_content_layout_applicable() => {
                return Err(LiveSessionError::Mobject(
                    "move_to cannot use an effective layout with render-content overrides".into(),
                ));
            }
            Ok(observed) if observed.object.transform != authored_transform => {
                return Err(LiveSessionError::Mobject(
                    "move_to cannot compose with an active effective affine driver".into(),
                ));
            }
            Ok(_) | Err(ExecutionSessionPublicationError::UnknownObject(_)) => {}
            Err(error) => return Err(error.into()),
        }
        drop(store);
        // A detached target has no execution row. Its authored state was created
        // through this session, so this is the exact coherent layout basis.
        let layout = self.layout_at_transform(mobject, authored_transform, publication)?;
        let mut translation = authored.transform.translation;
        translation.x =
            authoring_render_f64("move_to translation.x", translation.x + x - layout.center.0)
                .map_err(LiveSessionError::Mobject)?;
        translation.y =
            authoring_render_f64("move_to translation.y", translation.y + y - layout.center.1)
                .map_err(LiveSessionError::Mobject)?;
        self.set_property(mobject, SemanticObjectProperty::Translation, translation)
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
        let x = authoring_render_f64("scale.x", x).map_err(LiveSessionError::Mobject)?;
        let y = authoring_render_f64("scale.y", y).map_err(LiveSessionError::Mobject)?;
        let mut scale = self.authored(mobject)?.transform.scale;
        scale.x *= x;
        scale.y *= y;
        scale
            .lower_xy_f32()
            .map_err(|error| LiveSessionError::Mobject(error.to_string()))?;
        self.set_property(mobject, SemanticObjectProperty::Scale, scale)
    }

    /// Add a center-relative affine rotation through the shared live
    /// transaction. Pivot/layout rotation remains outside the bounded ordinary
    /// affine facade.
    pub fn rotate(
        &mut self,
        mobject: &Mobject,
        angle: f64,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        let angle = authoring_render_f64("rotation", angle).map_err(LiveSessionError::Mobject)?;
        let rotation = self.authored(mobject)?.transform.rotation_z + angle;
        let rotation =
            authoring_render_f64("rotation result", rotation).map_err(LiveSessionError::Mobject)?;
        self.set_property(mobject, SemanticObjectProperty::RotationZ, rotation)
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

    fn edit_style(
        &mut self,
        mobject: &Mobject,
        edit: impl FnOnce(&mut SemanticStyle) -> Result<(), String>,
    ) -> Result<SemanticMutationTransactionResult, LiveSessionError> {
        self.require_mobject(mobject)?;
        let mut style = mobject.state().map_err(LiveSessionError::Mobject)?.style;
        edit(&mut style).map_err(LiveSessionError::Mobject)?;
        self.replace_style(mobject, style)
    }

    fn require_mobject(&self, mobject: &Mobject) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, mobject.store()) {
            return Err(LiveSessionError::ForeignMobjectStore);
        }
        mobject.validate().map_err(LiveSessionError::Mobject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallbackAdvance, ExecutionSessionCreateError, Scene};
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
        callbacks.apply(&mut scene.store().borrow_mut()).unwrap();

        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let revision = live.session.publication_context().scene_revision();
        let mut overlay = match live.session.advance_to_callback_barrier(0.0).unwrap() {
            CallbackAdvance::HostRequired { overlay, .. } => overlay,
            CallbackAdvance::Ready(_) => panic!("time-zero callback phase must be required"),
        };
        assert!(matches!(
            live.target_editor(&circle),
            Err(LiveSessionError::Mobject(_))
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
        style.fill = Some(Color::rgb(1.0, 0.0, 0.0));
        style.opacity = 0.5;
        overlay.set_style(circle.node_id(), style).unwrap();
        live.session
            .commit_required_callback_phase(overlay.finish())
            .unwrap();

        live.session.take_frame_changes();
        let target = live.target_editor(&circle).unwrap();
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
            Err(LiveSessionError::Mobject(_))
        ));
        assert_eq!(live.session.publication_context(), before);
        assert_eq!(
            live.authored(&circle).unwrap().transform.translation,
            SemanticVec3::ZERO
        );
    }

    #[test]
    fn live_primitive_creation_is_detached_atomic_and_admits_locally() {
        let mut scene = Scene::new();
        let anchor = scene.circle(0.5).unwrap();
        scene.add(&anchor).unwrap();
        let mut session = scene.execution_session().unwrap();
        let mut live = scene.live(&mut session);
        let before = live.session.publication_context();
        live.session.take_frame_changes();

        let mut invalid = crate::ManimPrimitiveOptions::circle(0.25).unwrap();
        assert!(invalid.set_stroke_width(-0.1).is_err());
        assert_eq!(live.session.publication_context(), before);
        assert!(live.session.take_frame_changes().is_empty());

        let mut options = crate::ManimPrimitiveOptions::circle(0.25).unwrap();
        options.set_translation(2.0, -1.0).unwrap();
        options.set_fill(0.0, 0.4, 1.0, 0.6).unwrap();
        let circle = live.create_manim_primitive(options).unwrap();
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
        let before_nodes = circle.store().borrow().len();
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
        assert_eq!(circle.store().borrow().len(), before_nodes + 3);
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
        let before_nodes = circle.store().borrow().len();
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
        assert_eq!(circle.store().borrow().len(), before_nodes);
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
    }

    #[test]
    fn invalid_detached_affine_removal_does_not_admit_or_publish() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();
        let before_nodes = square.store().borrow().len();

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
        assert_eq!(square.store().borrow().len(), before_nodes);
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
    fn uncreate_rejects_asymmetric_rate_before_admission() {
        let scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        let before_nodes = square.store().borrow().len();
        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let before = session.publication_context();

        let result = scene.live(&mut session).declare_and_activate_uncreate(
            &square,
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::RushInto),
        );

        assert!(matches!(
            result,
            Err(LiveSessionError::Activation(
                ExecutionSessionAnimationError::CreateTarget {
                    error: ExecutionSessionCreateError::UnsupportedUncreateRateFunction(
                        RateFunction::RushInto
                    ),
                    ..
                }
            ))
        ));
        assert_eq!(session.publication_context(), before);
        assert_eq!(square.store().borrow().len(), before_nodes);
        assert!(session.frame().objects.is_empty());
        assert!(session.take_frame_changes().is_empty());
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
        let before_nodes = left.store().borrow().len();

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
        assert_eq!(left.store().borrow().len(), before_nodes + 5);
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
    fn prepared_sequence_uses_mapped_boundaries_and_releases_disjoint_style_channels() {
        let mut scene = Scene::new();
        let circle = scene.circle(1.0).unwrap();
        let mut fill_target = circle.target_editor().unwrap();
        fill_target.set_fill(1.0, 0.0, 0.0, 0.4).unwrap();
        let mut opacity_target = circle.target_editor().unwrap();
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
        let before_nodes = circle.store().borrow().len();
        let children = [
            TransformToRequest::new(&circle, &first_target, AnimationOptions::new()),
            TransformToRequest::new(&circle, &second_target, AnimationOptions::new()),
        ];

        let result = scene
            .live(&mut session)
            .declare_and_activate_transform_composition(
                SemanticAnimationCompositionKind::Sequence,
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
        assert_eq!(circle.store().borrow().len(), before_nodes);
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
        let before_nodes = square.store().borrow().len();
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
        assert_eq!(square.store().borrow().len(), before_nodes);
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
        let before_nodes = line.store().borrow().len();

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
        assert_eq!(line.store().borrow().len(), before_nodes);
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
        assert_eq!(session.frame().objects.len(), 4);
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
            live.declare_and_activate_fade(&fading, SemanticFadeDirection::Out, options)
                .unwrap()
        };
        {
            let mut live = scene.live(&mut session);
            live.advance_segment_to(fade_out, 1.5).unwrap();
            assert_eq!(live.effective(&fading).unwrap().appearance, 0.5);
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
