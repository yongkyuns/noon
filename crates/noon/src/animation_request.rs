//! Owned animation authoring requests over the canonical composition transaction.
//!
//! Requests retain handles and method arguments only. They allocate no semantic
//! identities and capture no target state until play admits the complete request.

use crate::execution_session::SemanticCompositionRequest;
use crate::semantic_mobject::{
    authoring_render_f64, edit_color, edit_fill, edit_object_opacity, rotate_affine_about_point,
    scale_state_about_center, state_center,
};
use crate::{
    CompositeEntryHandle, ExecutionSegment, LiveSession, LiveSessionError, Mobject, MobjectFamily,
    NoisePaint,
};
use std::sync::Arc;

mod group;
pub(crate) use group::{
    fixed_family_method_appearance, has_graph_members, is_method_family, is_ordinary_method_family,
    matching_method_family_paint, paired_planar_shift, supports_geometry_method_target,
    supports_layout_shift_target, FamilyMethodTarget, MethodFamilyAppearance,
};
use noon_core::{
    AnimationOptions, Color, SemanticAnimationCompositionKind, SemanticFadeDirection,
    SemanticFadeEndpoint, SemanticOrientation, SemanticTransformInterpolation, Vec2,
};

/// An owned, inert request accepted by scene playback.
///
/// Converting a tuple, array, or vector of requests creates a parallel composition.
/// All children are validated and published atomically by the shared animation
/// compiler. This value owns no scene state, semantic identity, or playback clock.
#[derive(Clone, Debug)]
#[must_use = "animation requests do nothing until passed to scene.play"]
pub struct AnimationRequest {
    kind: RequestKind,
}

#[derive(Clone, Debug)]
enum RequestKind {
    Animate(Animate),
    Create(Create),
    FadeOut(FadeOut),
    RadialReveal(Box<RadialReveal>),
    Parallel {
        children: Vec<AnimationRequest>,
        options: AnimationOptions,
    },
}

impl AnimationRequest {
    /// Supply options for this leaf or parallel composition.
    ///
    /// Values remain inert until the ordinary shared option resolver validates
    /// them at play admission. Play-level options retain their usual precedence.
    pub fn with_options(mut self, options: AnimationOptions) -> Self {
        match &mut self.kind {
            RequestKind::Animate(request) => request.options = options,
            RequestKind::Create(request) => request.options = options,
            RequestKind::FadeOut(request) => request.options = options,
            RequestKind::RadialReveal(request) => request.options = options,
            RequestKind::Parallel {
                options: current, ..
            } => *current = options,
        }
        self
    }

    fn parallel(children: Vec<Self>) -> Self {
        Self {
            kind: RequestKind::Parallel {
                children,
                options: AnimationOptions::new(),
            },
        }
    }

    fn prepare(
        self,
        live: &LiveSession<'_>,
    ) -> Result<SemanticCompositionRequest, LiveSessionError> {
        Ok(match self.kind {
            RequestKind::Animate(request) => request.prepare(live)?,
            RequestKind::Create(request) => {
                live.require_mobject(&request.target)?;
                SemanticCompositionRequest::Create {
                    target: request.target.node_id(),
                    options: request.options,
                }
            }
            RequestKind::FadeOut(request) => {
                live.require_mobject(&request.target)?;
                SemanticCompositionRequest::Fade {
                    target: request.target.node_id(),
                    direction: SemanticFadeDirection::Out,
                    endpoint: SemanticFadeEndpoint::default(),
                    options: request.options,
                }
            }
            RequestKind::RadialReveal(request) => {
                live.require_membership_root(request.target.as_target())?;
                SemanticCompositionRequest::Dissolve {
                    target: request.target.node_id(),
                    direction: request.direction,
                    style: noon_core::DissolveStyle::radial(request.update, request.edge)
                        .map_err(crate::AuthoringError::from)?,
                    options: request.options,
                }
            }
            RequestKind::Parallel { children, options } => {
                SemanticCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children: children
                        .into_iter()
                        .map(|child| child.prepare(live))
                        .collect::<Result<_, _>>()?,
                    options,
                }
            }
        })
    }
}

/// Convert an owned animation, or a collection of parallel animations, to a play request.
pub trait IntoAnimationRequest {
    fn into_animation_request(self) -> AnimationRequest;
}

impl<T: Into<AnimationRequest>> IntoAnimationRequest for T {
    fn into_animation_request(self) -> AnimationRequest {
        self.into()
    }
}

/// Group owned requests to play together through the shared composition scheduler.
///
/// Tuples support different animation types; arrays and vectors support any one
/// request type, including [`AnimationRequest`] for heterogeneous collections.
pub fn parallel(children: impl IntoAnimationRequest) -> AnimationRequest {
    let request = children.into_animation_request();
    if matches!(request.kind, RequestKind::Parallel { .. }) {
        request
    } else {
        AnimationRequest::parallel(vec![request])
    }
}

impl<T: IntoAnimationRequest, const N: usize> From<[T; N]> for AnimationRequest {
    fn from(children: [T; N]) -> Self {
        Self::parallel(
            children
                .into_iter()
                .map(IntoAnimationRequest::into_animation_request)
                .collect(),
        )
    }
}

impl<T: IntoAnimationRequest> From<Vec<T>> for AnimationRequest {
    fn from(children: Vec<T>) -> Self {
        Self::parallel(
            children
                .into_iter()
                .map(IntoAnimationRequest::into_animation_request)
                .collect(),
        )
    }
}

macro_rules! tuple_request {
    ($($type:ident $value:ident),+) => {
        impl<$($type: IntoAnimationRequest),+> From<($($type,)+)> for AnimationRequest {
            fn from(($($value,)+): ($($type,)+)) -> Self {
                Self::parallel(vec![$($value.into_animation_request()),+])
            }
        }
    };
}

tuple_request!(A a, B b);
tuple_request!(A a, B b, C c);
tuple_request!(A a, B b, C c, D d);
tuple_request!(A a, B b, C c, D d, E e);
tuple_request!(A a, B b, C c, D d, E e, F f);
tuple_request!(A a, B b, C c, D d, E e, F f, G g);
tuple_request!(A a, B b, C c, D d, E e, F f, G g, H h);

/// Chained edits describing one target for a planar object or ordinary vector family.
///
/// The source is captured from coherent effective state when play admits this
/// request. Retaining a builder across earlier plays therefore applies its edits
/// to the state after those plays. This differs from Python's eager `.animate`
/// target copy. Constructing or cloning this value never edits the source.
///
/// Edits are applied in chain order, then animated as one target. In particular,
/// [`Self::rotate`] uses point correspondence for vector content; it is not the
/// procedural angular path of an explicit Rotate animation. Families use the
/// existing family transform interpolation and a shared geometry-center pivot.
/// Fixed-membership planar families can enter on their first play. A play may
/// contain independent family targets alongside object animations. Animating a
/// shared member or the same family twice in one play is rejected atomically.
#[derive(Clone, Debug)]
#[must_use = "animation requests do nothing until passed to scene.play"]
pub struct Animate {
    source: CompositeEntryHandle,
    methods: Vec<Method>,
    object_opacity: Option<f64>,
    options: AnimationOptions,
}

#[derive(Clone, Debug)]
enum Method {
    Shift(Vec2),
    SetFill(Color),
    SetFillNoise(Arc<NoisePaint>),
    SetColor(Color),
    Scale(f64),
    Rotate(f64),
}

impl Mobject {
    /// Describe animated edits without borrowing the scene or mutating this object.
    ///
    /// Effective source state is captured when play admits the request, rather
    /// than when this builder is constructed. See [`Animate`] for capture and
    /// method-target interpolation semantics.
    pub fn animate(&self) -> Animate {
        Animate {
            source: CompositeEntryHandle::Mobject(self.clone()),
            methods: Vec::new(),
            object_opacity: None,
            options: AnimationOptions::new(),
        }
    }
}

impl MobjectFamily {
    /// Describe animated edits of a family of vector shapes, plain text, or MathTex.
    /// The source state is captured at play admission.
    /// Members retain their semantic identity and share scale/rotation pivots.
    /// A fully detached family enters the scene when the play is admitted.
    /// Shift and uniform scale preserve composed effects, including on entry.
    pub fn animate(&self) -> Animate {
        Animate {
            source: CompositeEntryHandle::Family(self.clone()),
            methods: Vec::new(),
            object_opacity: None,
            options: AnimationOptions::new(),
        }
    }
}

impl Animate {
    /// Translate by a world-space vector such as `RIGHT * 2.0`.
    pub fn shift(mut self, delta: Vec2) -> Self {
        self.methods.push(Method::Shift(delta));
        self
    }

    /// Set fill RGB and opacity from this RGBA color, preserving the stroke.
    pub fn set_fill(mut self, color: Color) -> Self {
        self.methods.push(Method::SetFill(color));
        self
    }

    /// Animate a local noise paint through the ordinary Fill channel.
    ///
    /// Fill opacity is preserved. Matching seed, octave count, lacunarity and
    /// ramp size are checked at play admission. This edit supports planar
    /// leaves, the same scope as persistent noise fills.
    pub fn set_fill_noise(mut self, paint: impl Into<Arc<NoisePaint>>) -> Self {
        self.methods.push(Method::SetFillNoise(paint.into()));
        self
    }

    /// Recolor fill and stroke while preserving their opacities.
    pub fn set_color(mut self, color: Color) -> Self {
        self.methods.push(Method::SetColor(color));
        self
    }

    /// Animate an object's overall opacity, or a group's final composited opacity.
    /// Group composite opacity can run together with a planar shift.
    pub fn set_object_opacity(mut self, opacity: f64) -> Self {
        self.object_opacity = Some(opacity);
        self
    }

    /// Scale uniformly about the object's current geometry center.
    pub fn scale(mut self, factor: f64) -> Self {
        self.methods.push(Method::Scale(factor));
        self
    }

    /// Rotate the target about its current geometry center, in radians.
    ///
    /// Vector points interpolate toward the rotated target. A half-turn can
    /// collapse points at its midpoint; a full turn need not visibly rotate.
    /// Use a procedural Rotate animation when an angular trajectory is intended.
    pub fn rotate(mut self, angle: f64) -> Self {
        self.methods.push(Method::Rotate(angle));
        self
    }

    /// Supply timing, easing, and other shared options for this animation.
    pub fn with_options(mut self, options: AnimationOptions) -> Self {
        self.options = options;
        self
    }

    fn prepare(
        self,
        live: &LiveSession<'_>,
    ) -> Result<SemanticCompositionRequest, LiveSessionError> {
        let source = match &self.source {
            CompositeEntryHandle::Family(family) => {
                if let Some(opacity) = self.object_opacity {
                    live.require_family(family)?;
                    if self
                        .methods
                        .iter()
                        .any(|method| !matches!(method, Method::Shift(_)))
                    {
                        return Err(LiveSessionError::Mobject(
                            "group composite opacity can be combined with a planar shift".into(),
                        ));
                    }
                    let motion = (!self.methods.is_empty())
                        .then(|| FamilyMethodTarget::capture(live, family, &self.methods, true))
                        .transpose()?
                        .map(Box::new);
                    return Ok(SemanticCompositionRequest::FamilyObjectOpacity {
                        source: family.node_id(),
                        opacity,
                        motion,
                        options: self.options,
                    });
                }
                return Ok(SemanticCompositionRequest::FamilyMethodTarget {
                    source: family.node_id(),
                    target: Box::new(FamilyMethodTarget::capture(
                        live,
                        family,
                        &self.methods,
                        false,
                    )?),
                    options: self.options,
                });
            }
            CompositeEntryHandle::Mobject(source) => source,
        };
        live.require_mobject(source)?;
        if source.has_members()? {
            return Err(LiveSessionError::Mobject(
                "owned animate requests currently require a single object without family members"
                    .into(),
            ));
        }
        let mut target = live.capture_mobject_state(source, false)?;
        if let Some(opacity) = self.object_opacity {
            edit_object_opacity(&mut target.style, opacity)?;
        }
        let source_rotation = target
            .transform
            .planar_rotation()
            .ok_or(crate::AuthoringError::NonFiniteObjectState)?;
        {
            let store = live.integration_store().borrow();
            for method in self.methods {
                match method {
                    Method::Shift(delta) => {
                        let x = authoring_render_f64("shift.x", f64::from(delta.x))?;
                        let y = authoring_render_f64("shift.y", f64::from(delta.y))?;
                        target.transform.translation.x = authoring_render_f64(
                            "translation.x",
                            target.transform.translation.x + x,
                        )?;
                        target.transform.translation.y = authoring_render_f64(
                            "translation.y",
                            target.transform.translation.y + y,
                        )?;
                    }
                    Method::SetColor(color) => edit_color(
                        &mut target.style,
                        f64::from(color.red),
                        f64::from(color.green),
                        f64::from(color.blue),
                        f64::from(color.alpha),
                    )?,
                    Method::SetFill(color) => edit_fill(
                        &mut target.style,
                        f64::from(color.red),
                        f64::from(color.green),
                        f64::from(color.blue),
                        f64::from(color.alpha),
                    )?,
                    Method::SetFillNoise(paint) => {
                        target.style.fill = Some(noon_core::SemanticPaint::Noise(paint));
                    }
                    Method::Scale(factor) => {
                        let center = state_center(&store, &target)?;
                        scale_state_about_center(&store, &mut target, factor, factor, center)?;
                    }
                    Method::Rotate(angle) => {
                        let center = state_center(&store, &target)?;
                        let (translation, rotation) = rotate_affine_about_point(
                            (
                                target.transform.translation.x,
                                target.transform.translation.y,
                            ),
                            target
                                .transform
                                .planar_rotation()
                                .ok_or(crate::AuthoringError::NonFiniteObjectState)?,
                            angle,
                            center,
                        )?;
                        target.transform.translation.x = translation.0;
                        target.transform.translation.y = translation.1;
                        target.transform.orientation = SemanticOrientation::Planar(rotation);
                    }
                }
            }
        }
        let target_rotation = target
            .transform
            .planar_rotation()
            .ok_or(crate::AuthoringError::NonFiniteObjectState)?;
        // Match the method-target distinction used by the Python frontend. Text,
        // image, and mesh content retain the existing affine content boundary.
        let rotation_changed = (source_rotation - target_rotation).abs()
            > 1e-12_f64.max(1e-9 * source_rotation.abs().max(target_rotation.abs()));
        let interpolation =
            if rotation_changed && !live.requires_content_boundary((source).into())? {
                SemanticTransformInterpolation::PointCorrespondence
            } else {
                SemanticTransformInterpolation::Affine
            };
        Ok(SemanticCompositionRequest::MethodTarget {
            source: source.node_id(),
            target_state: Box::new(target),
            interpolation,
            options: self.options,
        })
    }
}

impl From<Animate> for AnimationRequest {
    fn from(request: Animate) -> Self {
        Self {
            kind: RequestKind::Animate(request),
        }
    }
}

/// Reveal an object's vector geometry, admitting a detached object atomically.
#[derive(Clone, Debug)]
#[must_use = "animation requests do nothing until passed to scene.play"]
pub struct Create {
    target: Mobject,
    options: AnimationOptions,
}

impl Create {
    pub fn new(target: &Mobject) -> Self {
        Self {
            target: target.clone(),
            options: AnimationOptions::new(),
        }
    }

    pub fn with_options(mut self, options: AnimationOptions) -> Self {
        self.options = options;
        self
    }
}

impl From<Create> for AnimationRequest {
    fn from(request: Create) -> Self {
        Self {
            kind: RequestKind::Create(request),
        }
    }
}

/// Fade an admitted object and remove its scene membership at completion.
#[derive(Clone, Debug)]
#[must_use = "animation requests do nothing until passed to scene.play"]
pub struct FadeOut {
    target: Mobject,
    options: AnimationOptions,
}

impl FadeOut {
    pub fn new(target: &Mobject) -> Self {
        Self {
            target: target.clone(),
            options: AnimationOptions::new(),
        }
    }

    pub fn with_options(mut self, options: AnimationOptions) -> Self {
        self.options = options;
        self
    }
}

impl From<FadeOut> for AnimationRequest {
    fn from(request: FadeOut) -> Self {
        Self {
            kind: RequestKind::FadeOut(request),
        }
    }
}

/// Reveal an object or composed family through an expanding radial iris.
///
/// A target-only request enters detached content and uses a clean edge. `out()`
/// removes content through an expanding hole, with ordinary lifecycle options.
/// Values remain inert until `scene.play`; one canonical mask is reused across
/// repeated gestures and persistent `set_reveal` edits.
#[derive(Clone, Debug)]
#[must_use = "animation requests do nothing until passed to scene.play"]
pub struct RadialReveal {
    target: CompositeEntryHandle,
    direction: noon_core::DissolveDirection,
    update: noon_core::EffectUpdate,
    edge: Option<noon_core::RadialEdge>,
    options: AnimationOptions,
}
impl RadialReveal {
    pub fn new<'a>(target: impl Into<crate::MobjectTarget<'a>>) -> Self {
        let target = match target.into() {
            crate::MobjectTarget::Object(value) => CompositeEntryHandle::Mobject(value.clone()),
            crate::MobjectTarget::Family(value) => CompositeEntryHandle::Family(value.clone()),
        };
        Self {
            target,
            direction: noon_core::DissolveDirection::In,
            update: noon_core::EffectUpdate::default(),
            edge: None,
            options: AnimationOptions::new(),
        }
    }
    pub fn out(mut self) -> Self {
        self.direction = noon_core::DissolveDirection::Out;
        self
    }
    pub fn direction(mut self, direction: noon_core::DissolveDirection) -> Self {
        self.direction = direction;
        self
    }
    pub fn edge(mut self, edge: noon_core::RadialEdge) -> Self {
        self.edge = Some(edge);
        self
    }
    /// Advanced fields are checked at play admission by the same Rust operation
    /// as the language wrappers; omitted fields preserve the existing style.
    pub fn parameters(mut self, update: noon_core::EffectUpdate) -> Self {
        self.update = update;
        self
    }
    pub fn with_options(mut self, options: AnimationOptions) -> Self {
        self.options = options;
        self
    }
}
impl From<RadialReveal> for AnimationRequest {
    fn from(request: RadialReveal) -> Self {
        Self {
            kind: RequestKind::RadialReveal(Box::new(request)),
        }
    }
}

/// Prepare every child without publication, then enter the one canonical atomic
/// composition path. In particular, do not allocate committed target editors.
pub(crate) fn activate(
    live: &mut LiveSession<'_>,
    request: AnimationRequest,
    mut options: AnimationOptions,
) -> Result<ExecutionSegment, LiveSessionError> {
    let mut request = request.prepare(live)?;
    if let SemanticCompositionRequest::FamilyObjectOpacity {
        motion: Some(_),
        options: group_options,
        ..
    } = &mut request
    {
        // Resolve play overrides before splitting the method into its two channels.
        *group_options = group_options.with_overrides(std::mem::take(&mut options));
    }
    live.declare_and_activate_semantic_composition(&request, options)
}

#[cfg(test)]
mod tests;
