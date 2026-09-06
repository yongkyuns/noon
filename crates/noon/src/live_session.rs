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
    DeclaredAnimation, EffectiveSemanticObject, ExecutionSegment, ExecutionSegmentCompletionError,
    ExecutionSegmentError, ExecutionSegmentState, ExecutionSession, ExecutionSessionAnimationError,
    ExecutionSessionPublicationError, Mobject,
};
use noon_core::{
    Bounds2D64, PublicationContext, SemanticMutationTransaction, SemanticMutationTransactionResult,
    SemanticNodeId, SemanticObjectProperty, SemanticObjectState, SemanticSignalValue,
    SemanticStore, SemanticStyle, Style, Transform2D,
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

/// Errors while a semantic handle is used through a live execution session.
#[derive(Debug)]
pub enum LiveSessionError {
    ForeignMobjectStore,
    Mobject(String),
    Animation(String),
    Activation(ExecutionSessionAnimationError),
    Segment(ExecutionSegmentError),
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
        let state = source.state().map_err(LiveSessionError::Mobject)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(noon_core::SemanticNodeCreation::object(state));
        let result = self.apply(transaction)?;
        let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
            unreachable!("one prepared target copy has one exact semantic impact")
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
            .map_err(|error| LiveSessionError::Animation(error.to_string()))
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
    use crate::Scene;
    use noon_core::{AnimationOptions, Color, RateFunction, SemanticPaint, SemanticVec3};

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
                ExecutionSessionAnimationError::PreparedPayload(_)
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
}
