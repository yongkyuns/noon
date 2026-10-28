use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Upper bound for one activation-time retained path-motion snapshot.
pub const MAX_PATH_MOTION_COMMANDS: usize = 65_536;

/// Derived frame for a prepared point morph, separate from authored TRS.
///
/// Endpoint translations do not belong in immutable path coordinates. Sampling
/// this frame with the morph alpha preserves world-space point interpolation
/// while equivalent shape pairs reuse the same geometry across independent moves.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MorphRenderFrame {
    pub from: crate::Transform2D,
    pub to_translation: crate::Vec2,
}

impl MorphRenderFrame {
    pub const fn fixed(transform: crate::Transform2D) -> Self {
        Self {
            from: transform,
            to_translation: transform.translation,
        }
    }

    pub fn sample(self, alpha: f32) -> crate::Transform2D {
        let translation = if alpha <= 0.0 {
            self.from.translation
        } else if alpha >= 1.0 {
            self.to_translation
        } else {
            self.from.translation + (self.to_translation - self.from.translation) * alpha
        };
        crate::Transform2D {
            translation,
            ..self.from
        }
    }
}

use crate::{
    object_state::{validate_geometry, validate_style, validate_transform},
    CompositionTimeMap, CompositionTimeMapError, ObjectId, ObjectStateError, ObjectStateField,
    TrackId, Vec2,
};
use crate::{GeometryRef, Style, Transform2D};

/// Language-neutral animation rate functions shared by every authoring frontend.
///
/// The Manim-compatible variants reproduce Manim Community's deterministic
/// built-ins without requiring a Python callback during playback. Noon's previous
/// cubic easing remains available as an explicit low-level compatibility option.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateFunction {
    Linear,
    #[default]
    Smooth,
    Smoothstep,
    Smootherstep,
    Smoothererstep,
    SlowInto,
    DoubleSmooth,
    Lingering,
    /// Complemented reversed Lingering used by shared reverse Create lowering.
    MirroredLingering,
    RushInto,
    RushFrom,
    ThereAndBack,
    EaseInOutCubic,
    EaseInElastic,
    EaseOutElastic,
    EaseInOutElastic,
    EaseInBounce,
    EaseOutBounce,
    EaseInOutBounce,
    EaseInBack,
    EaseOutBack,
    EaseInOutBack,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,

    /// Hold the source value at normalized progress 0, then switch to the target.
    StepStart,
    /// Hold the source value until normalized progress reaches 1, then switch.
    StepEnd,
}

impl RateFunction {
    /// Whether this rate keeps normalized progress within the unit interval.
    /// Elastic and back rates intentionally overshoot; bounded parameter domains
    /// such as glow must reject those leaf rates rather than extrapolate values.
    pub const fn preserves_unit_interval(self) -> bool {
        match self {
            Self::EaseInElastic
            | Self::EaseOutElastic
            | Self::EaseInOutElastic
            | Self::EaseInBack
            | Self::EaseOutBack
            | Self::EaseInOutBack => false,
            Self::Linear
            | Self::Smooth
            | Self::Smoothstep
            | Self::Smootherstep
            | Self::Smoothererstep
            | Self::SlowInto
            | Self::DoubleSmooth
            | Self::Lingering
            | Self::MirroredLingering
            | Self::RushInto
            | Self::RushFrom
            | Self::ThereAndBack
            | Self::EaseInOutCubic
            | Self::EaseInBounce
            | Self::EaseOutBounce
            | Self::EaseInOutBounce
            | Self::EaseInExpo
            | Self::EaseOutExpo
            | Self::EaseInOutExpo
            | Self::EaseInCirc
            | Self::EaseOutCirc
            | Self::EaseInOutCirc
            | Self::EaseInSine
            | Self::EaseOutSine
            | Self::EaseInOutSine
            | Self::EaseInQuad
            | Self::EaseOutQuad
            | Self::EaseInOutQuad
            | Self::EaseInCubic
            | Self::EaseOutCubic
            | Self::EaseInQuart
            | Self::EaseOutQuart
            | Self::EaseInOutQuart
            | Self::EaseInQuint
            | Self::EaseOutQuint
            | Self::EaseInOutQuint
            | Self::StepStart
            | Self::StepEnd => true,
        }
    }

    /// Evaluate a normalized animation progress value.
    pub fn evaluate(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::Smooth => manim_smooth(progress),
            // Evaluate the polynomial and new monotone rates once in f64,
            // then project to the planar track's existing f32 precision.
            Self::Smoothstep
            | Self::Smootherstep
            | Self::Smoothererstep
            | Self::SlowInto
            | Self::DoubleSmooth
            | Self::Lingering
            | Self::MirroredLingering
            | Self::EaseInElastic
            | Self::EaseOutElastic
            | Self::EaseInOutElastic
            | Self::EaseInBounce
            | Self::EaseOutBounce
            | Self::EaseInOutBounce
            | Self::EaseInBack
            | Self::EaseOutBack
            | Self::EaseInOutBack
            | Self::EaseInExpo
            | Self::EaseOutExpo
            | Self::EaseInOutExpo
            | Self::EaseInCirc
            | Self::EaseOutCirc
            | Self::EaseInOutCirc
            | Self::EaseInSine
            | Self::EaseOutSine
            | Self::EaseInOutSine
            | Self::EaseInQuad
            | Self::EaseOutQuad
            | Self::EaseInOutQuad
            | Self::EaseInCubic
            | Self::EaseOutCubic
            | Self::EaseInQuart
            | Self::EaseOutQuart
            | Self::EaseInOutQuart
            | Self::EaseInQuint
            | Self::EaseOutQuint
            | Self::EaseInOutQuint => self.evaluate_f64(f64::from(progress)) as f32,
            Self::RushInto => 2.0 * manim_smooth(progress / 2.0),
            Self::RushFrom => 2.0 * manim_smooth(progress / 2.0 + 0.5) - 1.0,
            Self::ThereAndBack => {
                let mirrored = if progress < 0.5 {
                    2.0 * progress
                } else {
                    2.0 * (1.0 - progress)
                };
                manim_smooth(mirrored)
            }
            Self::EaseInOutCubic => {
                if progress < 0.5 {
                    4.0 * progress * progress * progress
                } else {
                    1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
                }
            }
            Self::StepStart => {
                if progress <= 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Self::StepEnd => {
                if progress < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }

    /// Evaluate the same rate function in f64 for high-precision world tracks.
    pub fn evaluate_f64(self, progress: f64) -> f64 {
        let p = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => p,
            Self::Smooth => manim_smooth_f64(p),
            Self::Smoothstep => 3.0 * p.powi(2) - 2.0 * p.powi(3),
            Self::Smootherstep => 6.0 * p.powi(5) - 15.0 * p.powi(4) + 10.0 * p.powi(3),
            Self::Smoothererstep => {
                35.0 * p.powi(4) - 84.0 * p.powi(5) + 70.0 * p.powi(6) - 20.0 * p.powi(7)
            }
            Self::SlowInto => (1.0 - (1.0 - p) * (1.0 - p)).sqrt(),
            Self::Lingering => (p / 0.8).min(1.0),
            Self::MirroredLingering => 1.0 - Self::Lingering.evaluate_f64(1.0 - p),
            Self::DoubleSmooth => {
                if p < 0.5 {
                    0.5 * manim_smooth_f64(2.0 * p)
                } else {
                    0.5 * (1.0 + manim_smooth_f64(2.0 * p - 1.0))
                }
            }
            Self::EaseInElastic => {
                let c4 = 2.0 * std::f64::consts::PI / 3.0;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else {
                    -2.0_f64.powf(10.0 * p - 10.0) * ((p * 10.0 - 10.75) * c4).sin()
                }
            }
            Self::EaseOutElastic => {
                let c4 = 2.0 * std::f64::consts::PI / 3.0;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else {
                    2.0_f64.powf(-10.0 * p) * ((p * 10.0 - 0.75) * c4).sin() + 1.0
                }
            }
            Self::EaseInOutElastic => {
                let c5 = 2.0 * std::f64::consts::PI / 4.5;
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else if p < 0.5 {
                    -(2.0_f64.powf(20.0 * p - 10.0) * ((20.0 * p - 11.125) * c5).sin()) / 2.0
                } else {
                    2.0_f64.powf(-20.0 * p + 10.0) * ((20.0 * p - 11.125) * c5).sin() / 2.0 + 1.0
                }
            }
            Self::EaseInBounce => 1.0 - manim_bounce_out(1.0 - p),
            Self::EaseOutBounce => manim_bounce_out(p),
            Self::EaseInOutBounce => {
                if p < 0.5 {
                    (1.0 - manim_bounce_out(1.0 - 2.0 * p)) / 2.0
                } else {
                    (1.0 + manim_bounce_out(2.0 * p - 1.0)) / 2.0
                }
            }
            Self::EaseInBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * p * p * p - c1 * p * p
            }
            Self::EaseOutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2)
            }
            Self::EaseInOutBack => {
                let c2 = 1.70158 * 1.525;
                if p < 0.5 {
                    (2.0 * p).powi(2) * ((c2 + 1.0) * 2.0 * p - c2) / 2.0
                } else {
                    ((2.0 * p - 2.0).powi(2) * ((c2 + 1.0) * (p * 2.0 - 2.0) + c2) + 2.0) / 2.0
                }
            }
            Self::EaseInExpo => {
                if p == 0.0 {
                    0.0
                } else {
                    2.0_f64.powf(10.0 * p - 10.0)
                }
            }
            Self::EaseOutExpo => {
                if p == 1.0 {
                    1.0
                } else {
                    1.0 - 2.0_f64.powf(-10.0 * p)
                }
            }
            Self::EaseInOutExpo => {
                if p == 0.0 {
                    0.0
                } else if p == 1.0 {
                    1.0
                } else if p < 0.5 {
                    2.0_f64.powf(20.0 * p - 10.0) / 2.0
                } else {
                    (2.0 - 2.0_f64.powf(-20.0 * p + 10.0)) / 2.0
                }
            }
            Self::EaseInCirc => 1.0 - (1.0 - p.powi(2)).sqrt(),
            Self::EaseOutCirc => (1.0 - (p - 1.0).powi(2)).sqrt(),
            Self::EaseInOutCirc => {
                if p < 0.5 {
                    (1.0 - (1.0 - (2.0 * p).powi(2)).sqrt()) / 2.0
                } else {
                    ((1.0 - (-2.0 * p + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }
            Self::EaseInSine => 1.0 - (p * std::f64::consts::PI / 2.0).cos(),
            Self::EaseOutSine => (p * std::f64::consts::PI / 2.0).sin(),
            Self::EaseInOutSine => -((std::f64::consts::PI * p).cos() - 1.0) / 2.0,
            Self::EaseInQuad => p * p,
            Self::EaseOutQuad => 1.0 - (1.0 - p) * (1.0 - p),
            Self::EaseInOutQuad => polynomial_in_out(p, 2),
            Self::EaseInCubic => p * p * p,
            Self::EaseOutCubic => 1.0 - (1.0 - p).powi(3),
            Self::EaseInQuart => p * p * p * p,
            Self::EaseOutQuart => 1.0 - (1.0 - p).powi(4),
            Self::EaseInOutQuart => polynomial_in_out(p, 4),
            Self::EaseInQuint => p * p * p * p * p,
            Self::EaseOutQuint => 1.0 - (1.0 - p).powi(5),
            Self::EaseInOutQuint => polynomial_in_out(p, 5),
            Self::RushInto => 2.0 * manim_smooth_f64(p / 2.0),
            Self::RushFrom => 2.0 * manim_smooth_f64(p / 2.0 + 0.5) - 1.0,
            Self::ThereAndBack => manim_smooth_f64(if p < 0.5 { 2.0 * p } else { 2.0 * (1.0 - p) }),
            Self::EaseInOutCubic => {
                if p < 0.5 {
                    4.0 * p * p * p
                } else {
                    1.0 - (-2.0 * p + 2.0).powi(3) / 2.0
                }
            }
            Self::StepStart => {
                if p <= 0.0 {
                    0.0
                } else {
                    1.0
                }
            }
            Self::StepEnd => {
                if p < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

fn polynomial_in_out(p: f64, degree: i32) -> f64 {
    if p < 0.5 {
        2.0_f64.powi(degree - 1) * p.powi(degree)
    } else {
        1.0 - (-2.0 * p + 2.0).powi(degree) / 2.0
    }
}

fn manim_bounce_out(t: f64) -> f64 {
    let n1 = 7.5625;
    let d1 = 2.75;
    if t < 1.0 / d1 {
        n1 * t * t
    } else if t < 2.0 / d1 {
        n1 * (t - 1.5 / d1) * (t - 1.5 / d1) + 0.75
    } else if t < 2.5 / d1 {
        n1 * (t - 2.25 / d1) * (t - 2.25 / d1) + 0.9375
    } else {
        n1 * (t - 2.625 / d1) * (t - 2.625 / d1) + 0.984375
    }
}

fn manim_smooth_f64(progress: f64) -> f64 {
    const INFLECTION: f64 = 10.0;
    let sigmoid = |value: f64| 1.0 / (1.0 + (-value).exp());
    let error = sigmoid(-INFLECTION / 2.0);
    ((sigmoid(INFLECTION * (progress - 0.5)) - error) / (1.0 - 2.0 * error)).clamp(0.0, 1.0)
}

fn manim_smooth(progress: f32) -> f32 {
    const INFLECTION: f32 = 10.0;
    let error = sigmoid(-INFLECTION / 2.0);
    ((sigmoid(INFLECTION * (progress - 0.5)) - error) / (1.0 - 2.0 * error)).clamp(0.0, 1.0)
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Presence,
    /// Exact painter priority, changed only by an instantaneous event.
    ZIndex,
    Transform,
    /// Full f64 world-space TRS animation for spatial execution rows.
    WorldTransform,
    /// Unwrapped f64 camera profile sampled atomically into pose and lens.
    CameraProfile,
    Position,
    Rotation,
    Scale,
    Fill,
    Stroke,
    StrokeWidth,
    Opacity,
    Appearance,
    Reveal,
    Morph,
    /// Independent appearance-only channels of one existing leaf attachment.
    EffectColor,
    EffectRadius,
    EffectIntensity,
    EffectOffset,
    EffectProgress,
    EffectSoftness,
    EffectSeed,
    EffectSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    Paint,
    Effect,
    Bool,
    ZIndex,
    Scalar,
    Vec2,
    Color,
    Object,
    WorldTransform,
    CameraProfile,
}

impl Property {
    pub fn accepts_value_kind(self, actual: ValueKind) -> bool {
        matches!((self, actual), (Self::Fill, ValueKind::Paint)) || self.value_kind() == actual
    }
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::EffectColor
            | Self::EffectRadius
            | Self::EffectIntensity
            | Self::EffectOffset
            | Self::EffectProgress
            | Self::EffectSoftness
            | Self::EffectSeed
            | Self::EffectSize => ValueKind::Effect,
            Self::Presence => ValueKind::Bool,
            Self::ZIndex => ValueKind::ZIndex,
            Self::Transform => ValueKind::Object,
            Self::WorldTransform => ValueKind::WorldTransform,
            Self::CameraProfile => ValueKind::CameraProfile,
            Self::Fill | Self::Stroke => ValueKind::Color,
            Self::Position | Self::Scale => ValueKind::Vec2,
            Self::Rotation
            | Self::StrokeWidth
            | Self::Opacity
            | Self::Appearance
            | Self::Reveal
            | Self::Morph => ValueKind::Scalar,
        }
    }

    pub const fn is_instant(self) -> bool {
        matches!(self, Self::Presence | Self::ZIndex)
    }
    pub const fn is_effect(self) -> bool {
        matches!(self.value_kind(), ValueKind::Effect)
    }
}

/// A typed endpoint for a single glow channel. Radius and intensity retain f64
/// precision; no renderer units, timing or attachment identity are inferred here.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum EffectTrackValue {
    Color(crate::Color),
    Radius(crate::EffectRadius),
    Intensity(f64),
    Offset(crate::EffectOffset),
    Progress(f64),
    Softness(f64),
    /// A constant discrete value for one scheduled interval. Different endpoint
    /// seeds cannot interpolate; successive intervals may choose another seed.
    Seed(u32),
    Size(crate::EffectRadius),
}

impl EffectTrackValue {
    pub fn from_definition(
        property: Property,
        definition: impl Into<crate::EffectDefinition>,
    ) -> Option<Self> {
        let definition = definition.into();
        match property {
            Property::EffectColor => definition.color().map(Self::Color),
            Property::EffectOffset => definition.as_shadow().map(|v| Self::Offset(v.offset())),
            Property::EffectRadius => definition.radius().map(Self::Radius),
            Property::EffectProgress => definition
                .as_noise_dissolve()
                .map(|v| Self::Progress(v.progress())),
            Property::EffectSoftness => definition
                .as_noise_dissolve()
                .map(|v| Self::Softness(v.softness())),
            Property::EffectSize => definition.as_pixelate().map(|v| Self::Size(v.size())),
            Property::EffectSeed => definition.as_noise_dissolve().map(|v| Self::Seed(v.seed())),
            Property::EffectIntensity => definition
                .as_glow()
                .map(|effect| Self::Intensity(effect.intensity())),
            _ => None,
        }
    }

    pub const fn property(self) -> Property {
        match self {
            Self::Color(_) => Property::EffectColor,
            Self::Radius(_) => Property::EffectRadius,
            Self::Intensity(_) => Property::EffectIntensity,
            Self::Offset(_) => Property::EffectOffset,
            Self::Progress(_) => Property::EffectProgress,
            Self::Softness(_) => Property::EffectSoftness,
            Self::Seed(_) => Property::EffectSeed,
            Self::Size(_) => Property::EffectSize,
        }
    }

    pub fn update(self) -> crate::EffectUpdate {
        match self {
            Self::Color(value) => crate::EffectUpdate::default().color(value),
            Self::Radius(value) => crate::EffectUpdate::default().radius(value),
            Self::Intensity(value) => crate::EffectUpdate::default().intensity(value),
            Self::Offset(value) => crate::EffectUpdate::default().offset(value),
            Self::Progress(value) => crate::EffectUpdate::default().progress(value),
            Self::Softness(value) => crate::EffectUpdate::default().softness(value),
            Self::Seed(value) => crate::EffectUpdate::default().seed(value),
            Self::Size(value) => crate::EffectUpdate::default().size(value),
        }
    }

    /// Use the same checked interpolation contract as semantic glow targets.
    /// The returned patch owns only this channel, not the whole definition.
    pub fn sample(self, to: Self, alpha: f64) -> Result<crate::EffectUpdate, TimelineError> {
        Ok(self.sample_value(to, alpha)?.update())
    }
    pub fn sample_value(self, to: Self, alpha: f64) -> Result<Self, TimelineError> {
        let invalid = || TimelineError::InvalidEffectValues(self.property());
        if self.property() != to.property() || !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(invalid());
        }
        self.update().validate().map_err(|_| invalid())?;
        to.update().validate().map_err(|_| invalid())?;
        let mix = crate::object_state::effect_parameters::interpolate;
        let result = match (self, to) {
            (Self::Color(a), Self::Color(b)) => Self::Color(crate::Color::rgba(
                mix(f64::from(a.red), f64::from(b.red), alpha) as f32,
                mix(f64::from(a.green), f64::from(b.green), alpha) as f32,
                mix(f64::from(a.blue), f64::from(b.blue), alpha) as f32,
                mix(f64::from(a.alpha), f64::from(b.alpha), alpha) as f32,
            )),
            (Self::Radius(a), Self::Radius(b)) if a.same_unit(b) => {
                Self::Radius(a.interpolate(b, alpha))
            }
            (Self::Size(a), Self::Size(b)) if a.same_unit(b) => Self::Size(a.interpolate(b, alpha)),
            (Self::Intensity(a), Self::Intensity(b)) => Self::Intensity(mix(a, b, alpha)),
            (Self::Progress(a), Self::Progress(b)) => Self::Progress(mix(a, b, alpha)),
            (Self::Softness(a), Self::Softness(b)) => Self::Softness(mix(a, b, alpha)),
            (Self::Seed(a), Self::Seed(b)) if a == b => Self::Seed(a),
            (Self::Offset(a), Self::Offset(b)) if a.same_unit(b) => {
                Self::Offset(a.interpolate(b, alpha))
            }
            _ => return Err(invalid()),
        };
        result.update().validate().map_err(|_| invalid())?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackValueEndpoint {
    From,
    To,
}

/// An exact renderer-independent endpoint for an execution transform track.
///
/// Semantic lowering resolves authored object references into this value before
/// runtime evaluation. It intentionally carries no authored object identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformTrackEndpoint {
    pub geometry: GeometryRef,
    pub transform: Transform2D,
    pub style: Style,
}

/// Serde-compatible f64 TRS endpoint. Quaternion components are stored in
/// `(w, x, y, z)` order and validated before a track is admitted.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldTransformTrackEndpoint {
    pub translation: crate::SemanticVec3,
    pub rotation: [f64; 4],
    pub scale: crate::SemanticVec3,
}

impl WorldTransformTrackEndpoint {
    pub fn from_world(value: crate::SemanticWorldTransform3D) -> Self {
        Self {
            translation: value.translation,
            rotation: value.rotation.components(),
            scale: value.scale,
        }
    }

    pub fn world(self) -> Option<crate::SemanticWorldTransform3D> {
        crate::SemanticWorldTransform3D::new(
            self.translation,
            crate::SemanticRotation3D::from_components(
                self.rotation[0],
                self.rotation[1],
                self.rotation[2],
                self.rotation[3],
            )?,
            self.scale,
        )
    }
}

impl TransformTrackEndpoint {
    pub fn new(geometry: GeometryRef) -> Self {
        Self {
            geometry,
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }
}

impl std::fmt::Display for TrackValueEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::From => "from",
            Self::To => "to",
        })
    }
}

/// The effective value left by one glow channel when its mapped leaf finishes.
/// This is independent of the leaf's amplitude/easing function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTrackEndBehavior {
    /// Keep the ordinary terminal sample selected by the leaf's rate function.
    #[default]
    Sample,
    /// Restore the captured source value, including for non-returning rates.
    RestoreSource,
}

impl EffectTrackEndBehavior {
    const fn is_sample(&self) -> bool {
        matches!(self, Self::Sample)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackValues {
    /// Continuous fill endpoints share the ordinary Fill channel with solids.
    Paint {
        transition: Arc<crate::PaintTransition>,
    },
    /// Generation is captured at activation; a reused object slot or attachment
    /// name never redirects an already prepared effect track.
    Effect {
        attachment: crate::SemanticNodeId,
        from: EffectTrackValue,
        to: EffectTrackValue,
        #[serde(default, skip_serializing_if = "EffectTrackEndBehavior::is_sample")]
        end_behavior: EffectTrackEndBehavior,
    },
    /// Exact finite f64 priorities; never lowered to shader precision or interpolated.
    ZIndex {
        from: f64,
        to: f64,
    },
    Bool {
        from: bool,
        to: bool,
    },
    Scalar {
        from: f32,
        to: f32,
    },
    Vec2 {
        from: Vec2,
        to: Vec2,
    },
    /// A deterministic Manim-style circular path for one 2D position channel.
    ///
    /// This remains renderer-independent execution data. The source and target
    /// endpoints are exact; `arc_angle` only changes interpolation between them.
    ArcVec2 {
        from: Vec2,
        to: Vec2,
        arc_angle: f64,
    },
    /// Runtime-prepared arc-length motion over one activation-time path snapshot.
    PathVec2 {
        path: Arc<crate::VectorPath>,
        path_transform: crate::Transform2D,
        target_center_offset: Vec2,
    },
    Color {
        from: Option<crate::Color>,
        to: Option<crate::Color>,
    },
    /// Renderer-independent geometry prepared by lowering for a scalar morph channel.
    ///
    /// This is execution data, not an authored object endpoint: semantic transform,
    /// style, identity, and completion remain in their owning channels and store.
    PreparedMorph {
        from: f32,
        to: f32,
        geometry: crate::GeometryRef,
        render_frame: Option<MorphRenderFrame>,
        /// Captured semantic frame of the source. Before activation, geometry
        /// must follow earlier affine drivers, including collapsed entrances.
        source_transform: crate::Transform2D,
    },
    Object {
        from: TransformTrackEndpoint,
        to: TransformTrackEndpoint,
    },
    WorldTransform {
        from: WorldTransformTrackEndpoint,
        to: WorldTransformTrackEndpoint,
    },
    CameraProfile {
        from: crate::ManimCamera3DProfile,
        to: crate::ManimCamera3DProfile,
        near: f64,
        far: f64,
    },
}

impl TrackValues {
    /// Endpoint used by ordinary subsequent activation captures. A restoring
    /// pulse releases to its source independently of easing; other channels keep
    /// the existing f32 terminal-rate contract.
    pub fn terminal_progress(&self, timing: TrackTiming) -> f32 {
        match self {
            Self::Effect {
                end_behavior: EffectTrackEndBehavior::RestoreSource,
                ..
            } => 0.0,
            _ => timing.terminal_progress(),
        }
    }

    /// f64-preserving terminal sampling for glow and spatial runtime channels.
    pub fn terminal_progress_f64(&self, timing: TrackTiming) -> f64 {
        match self {
            Self::Effect {
                end_behavior: EffectTrackEndBehavior::RestoreSource,
                ..
            } => 0.0,
            _ => timing.terminal_progress_f64(),
        }
    }

    pub const fn value_kind(&self) -> ValueKind {
        match self {
            Self::Paint { .. } => ValueKind::Paint,
            Self::Effect { .. } => ValueKind::Effect,
            Self::Bool { .. } => ValueKind::Bool,
            Self::ZIndex { .. } => ValueKind::ZIndex,
            Self::Scalar { .. } => ValueKind::Scalar,
            Self::Vec2 { .. } | Self::ArcVec2 { .. } | Self::PathVec2 { .. } => ValueKind::Vec2,
            Self::Color { .. } => ValueKind::Color,
            Self::PreparedMorph { .. } => ValueKind::Scalar,
            Self::Object { .. } => ValueKind::Object,
            Self::WorldTransform { .. } => ValueKind::WorldTransform,
            Self::CameraProfile { .. } => ValueKind::CameraProfile,
        }
    }

    fn validate_numeric_values(
        &self,
        object: ObjectId,
        property: Property,
    ) -> Result<(), TimelineError> {
        match self {
            Self::Effect { from, to, .. } => {
                if from.property() != property
                    || to.property() != property
                    || from.sample(*to, 0.0).is_err()
                {
                    return Err(TimelineError::InvalidEffectValues(property));
                }
                Ok(())
            }
            Self::ZIndex { from, to } if !from.is_finite() || !to.is_finite() => {
                Err(TimelineError::InvalidZIndexValues {
                    from: *from,
                    to: *to,
                })
            }
            Self::Scalar { from, to } if !from.is_finite() || !to.is_finite() => {
                Err(TimelineError::InvalidScalarValues {
                    property,
                    from: *from,
                    to: *to,
                })
            }
            Self::Scalar { from, to }
                if property == Property::StrokeWidth && (*from < 0.0 || *to < 0.0) =>
            {
                Err(TimelineError::InvalidStrokeWidthValues {
                    from: *from,
                    to: *to,
                })
            }
            Self::Vec2 { from, to } | Self::ArcVec2 { from, to, .. }
                if !from.x.is_finite()
                    || !from.y.is_finite()
                    || !to.x.is_finite()
                    || !to.y.is_finite() =>
            {
                Err(TimelineError::InvalidVec2Values {
                    property,
                    from: *from,
                    to: *to,
                })
            }
            Self::ArcVec2 { arc_angle, .. } if !arc_angle.is_finite() => {
                Err(TimelineError::InvalidPathArcValue {
                    property,
                    value: *arc_angle,
                })
            }
            Self::PathVec2 {
                path,
                path_transform,
                target_center_offset,
            } if property != Property::Position
                || !path.is_finite()
                || !path.has_drawable_segments()
                || path.retained_command_count() > MAX_PATH_MOTION_COMMANDS
                || !path_transform.translation.x.is_finite()
                || !path_transform.translation.y.is_finite()
                || !path_transform.rotation.is_finite()
                || !path_transform.scale.x.is_finite()
                || !path_transform.scale.y.is_finite()
                || !target_center_offset.x.is_finite()
                || !target_center_offset.y.is_finite()
                || !path.has_finite_transformed_output(*path_transform, *target_center_offset) =>
            {
                Err(TimelineError::InvalidPathMotionValues(property))
            }
            Self::Color { from, to } => {
                for (endpoint, color) in [
                    (TrackValueEndpoint::From, from),
                    (TrackValueEndpoint::To, to),
                ] {
                    if color.as_ref().is_some_and(|color| {
                        !color.red.is_finite()
                            || !color.green.is_finite()
                            || !color.blue.is_finite()
                            || !color.alpha.is_finite()
                    }) {
                        return Err(TimelineError::InvalidColorValue { property, endpoint });
                    }
                }
                Ok(())
            }
            Self::PreparedMorph {
                from,
                to,
                geometry,
                render_frame,
                source_transform,
            } => {
                if !from.is_finite() || !to.is_finite() {
                    return Err(TimelineError::InvalidScalarValues {
                        property,
                        from: *from,
                        to: *to,
                    });
                }
                validate_geometry(object, geometry).map_err(|error| {
                    invalid_object_track_value(property, TrackValueEndpoint::From, error)
                })?;
                validate_transform(object, *source_transform).map_err(|error| {
                    invalid_object_track_value(property, TrackValueEndpoint::From, error)
                })?;
                if let Some(frame) = render_frame {
                    validate_transform(object, frame.from).map_err(|error| {
                        invalid_object_track_value(property, TrackValueEndpoint::From, error)
                    })?;
                    validate_transform(object, frame.sample(1.0)).map_err(|error| {
                        invalid_object_track_value(property, TrackValueEndpoint::To, error)
                    })?;
                    validate_transform(object, frame.sample(0.5)).map_err(|error| {
                        invalid_object_track_value(property, TrackValueEndpoint::To, error)
                    })?;
                }
                Ok(())
            }
            Self::Object { from, to } => {
                validate_object_track_value(object, property, TrackValueEndpoint::From, from)?;
                validate_object_track_value(object, property, TrackValueEndpoint::To, to)
            }
            Self::WorldTransform { from, to } => {
                if property != Property::WorldTransform
                    || from.world().is_none()
                    || to.world().is_none()
                {
                    Err(TimelineError::InvalidWorldTransformValues)
                } else {
                    Ok(())
                }
            }
            Self::CameraProfile {
                from,
                to,
                near,
                far,
            } => {
                if property != Property::CameraProfile
                    || from.camera(*near, *far).is_none()
                    || to.camera(*near, *far).is_none()
                {
                    Err(TimelineError::InvalidCameraProfileValues)
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
}

fn validate_object_track_value(
    object: ObjectId,
    property: Property,
    endpoint: TrackValueEndpoint,
    snapshot: &TransformTrackEndpoint,
) -> Result<(), TimelineError> {
    validate_geometry(object, &snapshot.geometry)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))?;
    validate_transform(object, snapshot.transform)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))?;
    validate_style(object, snapshot.style)
        .map_err(|error| invalid_object_track_value(property, endpoint, error))
}

fn invalid_object_track_value(
    property: Property,
    endpoint: TrackValueEndpoint,
    error: ObjectStateError,
) -> TimelineError {
    let ObjectStateError { field, .. } = error;
    TimelineError::InvalidObjectValue {
        property,
        endpoint,
        field,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackTiming {
    pub start_time: f64,
    pub duration: f64,
    pub easing: RateFunction,
    /// Evaluate this leaf's rate function at `1 - alpha`, as Manim's
    /// `reverse_rate_function=True` does. Composition time maps remain outer
    /// to this leaf-local rate function.
    #[serde(default)]
    #[serde(skip_serializing_if = "is_false")]
    pub reverse_rate_function: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl TrackTiming {
    pub const fn new(start_time: f64, duration: f64, easing: RateFunction) -> Self {
        Self {
            start_time,
            duration,
            easing,
            reverse_rate_function: false,
        }
    }

    pub const fn instant(start_time: f64) -> Self {
        Self::new(start_time, 0.0, RateFunction::Linear)
    }

    pub const fn is_instant(self) -> bool {
        self.duration == 0.0
    }

    /// Apply the leaf-local rate function, including Manim's reverse input.
    pub fn evaluate_progress(self, alpha: f32) -> f32 {
        self.easing.evaluate(if self.reverse_rate_function {
            1.0 - alpha
        } else {
            alpha
        })
    }

    /// f64-preserving form used by spatial timelines.
    pub fn evaluate_progress_f64(self, alpha: f64) -> f64 {
        self.easing.evaluate_f64(if self.reverse_rate_function {
            1.0 - alpha
        } else {
            alpha
        })
    }

    /// The leaf's exact terminal interpolation progress.
    pub fn terminal_progress(self) -> f32 {
        self.evaluate_progress(1.0)
    }

    pub fn terminal_progress_f64(self) -> f64 {
        self.evaluate_progress_f64(1.0)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackDefinition {
    pub id: TrackId,
    pub object: ObjectId,
    pub property: Property,
    pub values: TrackValues,
    pub timing: TrackTiming,
    #[serde(default, skip_serializing_if = "CompositionTimeMap::is_identity")]
    pub time_map: CompositionTimeMap,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TimelineError {
    InvalidPaintValues,
    UnsupportedPaintRateFunction(RateFunction),
    InvalidEffectValues(Property),
    UnsupportedGlowRateFunction(RateFunction),
    InvalidEffectAttachment {
        object: ObjectId,
        attachment: crate::SemanticNodeId,
    },
    InvalidZIndexValues {
        from: f64,
        to: f64,
    },
    InvalidStartTime(f64),
    InvalidDuration(f64),
    InvalidInstantDuration {
        property: Property,
        duration: f64,
    },
    ValueTypeMismatch {
        property: Property,
        expected: ValueKind,
        actual: ValueKind,
    },
    PreparedMorphPropertyMismatch(Property),
    ArcVec2PropertyMismatch(Property),
    InvalidPathMotionValues(Property),
    InvalidScalarValues {
        property: Property,
        from: f32,
        to: f32,
    },
    InvalidStrokeWidthValues {
        from: f32,
        to: f32,
    },
    InvalidVec2Values {
        property: Property,
        from: Vec2,
        to: Vec2,
    },
    InvalidPathArcValue {
        property: Property,
        value: f64,
    },
    InvalidColorValue {
        property: Property,
        endpoint: TrackValueEndpoint,
    },
    InvalidObjectValue {
        property: Property,
        endpoint: TrackValueEndpoint,
        field: ObjectStateField,
    },
    InvalidWorldTransformValues,
    InvalidCameraProfileValues,
    InvalidCompositionTimeMap(CompositionTimeMapError),
    InstantTrackCannotUseTimeMap(Property),
}

impl std::fmt::Display for TimelineError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPaintValues => formatter.write_str("continuous paint track is outside the finite planar fill profile"),
            Self::UnsupportedPaintRateFunction(rate) => write!(formatter, "continuous paint tracks require a leaf rate bounded to [0, 1], got {rate:?}"),
            Self::InvalidEffectValues(property) => write!(formatter, "invalid or incompatible effect endpoints for {property:?}"),
            Self::UnsupportedGlowRateFunction(rate) => write!(formatter, "glow tracks require a leaf rate bounded to [0, 1], got {rate:?}"),
            Self::InvalidEffectAttachment { object, attachment } => write!(formatter, "glow track for {object:?} does not match attachment {attachment:?}"),
            Self::InvalidStartTime(value) => write!(formatter, "invalid start time {value}"),
            Self::InvalidDuration(value) => write!(formatter, "invalid duration {value}"),
            Self::InvalidInstantDuration { property, duration } => write!(
                formatter,
                "instant {property:?} track requires zero duration, got {duration}"
            ),
            Self::ValueTypeMismatch {
                property,
                expected,
                actual,
            } => write!(
                formatter,
                "value type mismatch for {property:?}: expected {expected:?}, got {actual:?}"
            ),
            Self::PreparedMorphPropertyMismatch(property) => write!(
                formatter,
                "prepared morph execution data cannot drive {property:?}"
            ),
            Self::ArcVec2PropertyMismatch(property) => write!(
                formatter,
                "curved vector execution data can only drive Position, not {property:?}"
            ),
            Self::InvalidPathMotionValues(property) => write!(
                formatter,
                "retained path motion requires finite 2D geometry and can only drive Position, not {property:?}"
            ),
            Self::InvalidZIndexValues { from, to } => write!(
                formatter,
                "painter priorities must be finite: {from} -> {to}"
            ),
            Self::InvalidScalarValues { property, from, to } => write!(
                formatter,
                "non-finite scalar values for {property:?}: from={from}, to={to}"
            ),
            Self::InvalidStrokeWidthValues { from, to } => write!(
                formatter,
                "stroke width values must be non-negative: from={from}, to={to}"
            ),
            Self::InvalidVec2Values { property, from, to } => write!(
                formatter,
                "non-finite vector values for {property:?}: from=({}, {}), to=({}, {})",
                from.x, from.y, to.x, to.y
            ),
            Self::InvalidPathArcValue { property, value } => {
                write!(formatter, "non-finite path arc for {property:?}: {value}")
            }
            Self::InvalidColorValue { property, endpoint } => write!(
                formatter,
                "non-finite color in {endpoint} value for {property:?}"
            ),
            Self::InvalidObjectValue {
                property,
                endpoint,
                field,
            } => write!(
                formatter,
                "non-finite {field} state in {endpoint} object value for {property:?}"
            ),
            Self::InvalidWorldTransformValues => formatter.write_str(
                "world transform track requires finite translation and scale and normalized quaternion endpoints",
            ),
            Self::InvalidCameraProfileValues => formatter.write_str(
                "camera-profile track requires valid unwrapped camera profiles and clipping planes",
            ),
            Self::InvalidCompositionTimeMap(error) => error.fmt(formatter),
            Self::InstantTrackCannotUseTimeMap(property) => write!(
                formatter,
                "instant {property:?} tracks cannot carry a composition time map"
            ),
        }
    }
}

impl std::error::Error for TimelineError {}

pub(crate) fn validate_track_timing(
    property: Property,
    timing: TrackTiming,
) -> Result<(), TimelineError> {
    validate_track_time_fields(timing)?;
    if property.is_instant() {
        if timing.duration != 0.0 {
            return Err(TimelineError::InvalidInstantDuration {
                property,
                duration: timing.duration,
            });
        }
    } else if timing.duration < 0.0 {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

pub(crate) fn validate_continuous_track_timing(timing: TrackTiming) -> Result<(), TimelineError> {
    validate_track_time_fields(timing)?;
    if timing.duration < 0.0 {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

fn validate_track_time_fields(timing: TrackTiming) -> Result<(), TimelineError> {
    if !timing.start_time.is_finite() {
        return Err(TimelineError::InvalidStartTime(timing.start_time));
    }
    if !timing.duration.is_finite() {
        return Err(TimelineError::InvalidDuration(timing.duration));
    }
    Ok(())
}

pub fn validate_track_definition(track: &TrackDefinition) -> Result<(), TimelineError> {
    if track.property.is_instant() && !track.time_map.is_identity() {
        validate_continuous_track_timing(track.timing)?;
    } else {
        validate_track_timing(track.property, track.timing)?;
    }
    let expected = track.property.value_kind();
    let actual = track.values.value_kind();
    if !track.property.accepts_value_kind(actual) {
        return Err(TimelineError::ValueTypeMismatch {
            property: track.property,
            expected,
            actual,
        });
    }
    if matches!(&track.values, TrackValues::PreparedMorph { .. })
        && track.property != Property::Morph
    {
        return Err(TimelineError::PreparedMorphPropertyMismatch(track.property));
    }
    if matches!(&track.values, TrackValues::ArcVec2 { .. }) && track.property != Property::Position
    {
        return Err(TimelineError::ArcVec2PropertyMismatch(track.property));
    }
    if matches!(&track.values, TrackValues::PathVec2 { .. }) && track.property != Property::Position
    {
        return Err(TimelineError::InvalidPathMotionValues(track.property));
    }
    track
        .values
        .validate_numeric_values(track.object, track.property)?;
    if matches!(track.values, TrackValues::Effect { .. })
        && !track.timing.easing.preserves_unit_interval()
    {
        return Err(TimelineError::UnsupportedGlowRateFunction(
            track.timing.easing,
        ));
    }
    if matches!(track.values, TrackValues::Paint { .. })
        && !track.timing.easing.preserves_unit_interval()
    {
        return Err(TimelineError::UnsupportedPaintRateFunction(
            track.timing.easing,
        ));
    }
    if matches!(
        track.values,
        TrackValues::Effect {
            end_behavior: EffectTrackEndBehavior::RestoreSource,
            ..
        }
    ) {
        if track.property != Property::EffectIntensity {
            return Err(TimelineError::InvalidEffectValues(track.property));
        }
        if track.timing.duration <= 0.0 {
            return Err(TimelineError::InvalidDuration(track.timing.duration));
        }
    }
    if track.timing.is_instant() && !track.property.is_instant() && !track.time_map.is_identity() {
        return Err(TimelineError::InstantTrackCannotUseTimeMap(track.property));
    }
    track
        .time_map
        .validate()
        .map_err(TimelineError::InvalidCompositionTimeMap)?;
    if track.property.is_instant() && !track.time_map.is_identity() {
        track
            .time_map
            .monotone_event_alpha()
            .map_err(TimelineError::InvalidCompositionTimeMap)?;
    }
    Ok(())
}

/// Resolve authored timing into the timing consumed by the execution scheduler.
///
/// A mapped discrete track carries its composition root interval during semantic
/// lowering. It is compiled to one instant boundary so playback and direct seek
/// use the same event scheduler and cannot replay a reversing time-map event.
pub fn resolve_track_timing(track: &TrackDefinition) -> Result<TrackTiming, TimelineError> {
    validate_track_definition(track)?;
    if !track.property.is_instant() || track.time_map.is_identity() {
        return Ok(track.timing);
    }
    let alpha = track
        .time_map
        .monotone_event_alpha()
        .map_err(TimelineError::InvalidCompositionTimeMap)?;
    Ok(TrackTiming::instant(
        track.timing.start_time + track.timing.duration * alpha,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CompositionTimeMapStep, GeometryRef, Style};

    fn timing() -> TrackTiming {
        TrackTiming::new(1.0, 2.0, RateFunction::Linear)
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn lingering_matches_original_values_and_shared_reverse_create_complement() {
        for line in include_str!("lingering_reference.csv")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .skip(1)
        {
            let mut fields = line.split(',');
            let progress = fields.next().unwrap().parse::<f64>().unwrap();
            let expected = fields.next().unwrap().parse::<f64>().unwrap();
            assert!((RateFunction::Lingering.evaluate_f64(progress) - expected).abs() < 1e-14);
            assert!(
                (RateFunction::Lingering.evaluate(progress as f32) - expected as f32).abs() < 1e-6
            );
            assert!(
                (1.0 - RateFunction::MirroredLingering.evaluate_f64(1.0 - progress) - expected)
                    .abs()
                    < 1e-14
            );
        }
        assert_eq!(RateFunction::Lingering.evaluate_f64(0.8), 1.0);
        assert_eq!(RateFunction::Lingering.evaluate_f64(0.9), 1.0);
        assert_eq!(RateFunction::MirroredLingering.evaluate_f64(0.1), 0.0);
    }

    #[test]
    fn manim_rate_functions_have_exact_endpoints_and_reference_values() {
        assert_eq!(RateFunction::Linear.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::Linear.evaluate(1.0), 1.0);
        assert_eq!(RateFunction::Smooth.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::Smooth.evaluate(1.0), 1.0);
        assert_close(RateFunction::Smooth.evaluate(0.25), 0.07010372);
        assert_close(RateFunction::Smooth.evaluate(0.5), 0.5);
        assert_close(RateFunction::Smooth.evaluate(0.75), 0.9298963);
        assert_close(
            RateFunction::RushInto.evaluate(0.5),
            2.0 * RateFunction::Smooth.evaluate(0.25),
        );
        assert_close(
            RateFunction::RushFrom.evaluate(0.5),
            2.0 * RateFunction::Smooth.evaluate(0.75) - 1.0,
        );
        assert_eq!(RateFunction::ThereAndBack.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::ThereAndBack.evaluate(0.5), 1.0);
        assert_eq!(RateFunction::ThereAndBack.evaluate(1.0), 0.0);
        assert_eq!(RateFunction::StepStart.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::StepStart.evaluate(f32::EPSILON), 1.0);
        assert_eq!(RateFunction::StepStart.evaluate(1.0), 1.0);
        assert_eq!(RateFunction::StepEnd.evaluate(0.0), 0.0);
        assert_eq!(RateFunction::StepEnd.evaluate(1.0 - f32::EPSILON), 0.0);
        assert_eq!(RateFunction::StepEnd.evaluate(1.0), 1.0);
    }

    #[test]
    fn polynomial_rates_match_pinned_values_at_both_track_precisions() {
        for (rate, quarter) in [
            (RateFunction::Smoothstep, 0.15625_f64),
            (RateFunction::Smootherstep, 0.103515625),
            (RateFunction::Smoothererstep, 0.070556640625),
        ] {
            for (progress, expected) in [
                (-1.0, 0.0),
                (0.0, 0.0),
                (0.25, quarter),
                (0.5, 0.5),
                (0.75, 1.0 - quarter),
                (1.0, 1.0),
                (2.0, 1.0),
            ] {
                assert_eq!(rate.evaluate_f64(progress), expected);
                assert_eq!(rate.evaluate(progress as f32), expected as f32);
            }
        }
    }

    fn assert_pinned_easing_samples(reference: &str, expected_count: usize) {
        let mut count = 0;
        for row in reference.lines().filter(|s| !s.starts_with('#')) {
            let values = row.split(',').collect::<Vec<_>>();
            let rate = RateFunction::from_semantic_id(values[0]).unwrap();
            let progress = values[1].parse::<f64>().unwrap();
            let expected = values[2].parse::<f64>().unwrap();
            assert!(
                (rate.evaluate_f64(progress) - expected).abs() < 1e-14,
                "{row}"
            );
            assert_eq!(rate.evaluate(progress as f32), expected as f32, "{row}");
            count += 1;
        }
        assert_eq!(count, expected_count);
    }

    #[test]
    fn elastic_bounce_easings_match_original_samples_in_both_precisions() {
        assert_pinned_easing_samples(include_str!("elastic_bounce_easing_reference.csv"), 1542);
        assert!(RateFunction::EaseInElastic.evaluate_f64(0.25) < 0.0);
        assert!(RateFunction::EaseOutElastic.evaluate_f64(0.125) > 1.0);
        for rate in [
            RateFunction::EaseInElastic,
            RateFunction::EaseOutElastic,
            RateFunction::EaseInOutElastic,
            RateFunction::EaseInBounce,
            RateFunction::EaseOutBounce,
            RateFunction::EaseInOutBounce,
        ] {
            assert_eq!(rate.evaluate_f64(0.0), 0.0);
            assert_eq!(rate.evaluate_f64(1.0), 1.0);
        }
    }

    #[test]
    fn back_easings_match_original_overshoot_in_both_precisions() {
        assert_pinned_easing_samples(include_str!("back_easing_reference.csv"), 99);
        assert!(RateFunction::EaseInBack.evaluate_f64(0.25) < 0.0);
        assert!(RateFunction::EaseOutBack.evaluate_f64(0.75) > 1.0);
        for rate in [
            RateFunction::EaseInBack,
            RateFunction::EaseOutBack,
            RateFunction::EaseInOutBack,
        ] {
            assert!(rate.evaluate_f64(0.0).abs() < 1e-14);
            assert!((rate.evaluate_f64(1.0) - 1.0).abs() < 1e-14);
        }
    }

    #[test]
    fn exponential_circular_easings_match_unchanged_manim_finite_samples() {
        assert_pinned_easing_samples(
            include_str!("exponential_circular_easing_reference.csv"),
            198,
        );
    }

    #[test]
    fn sine_easings_match_unchanged_manim_finite_samples() {
        assert_pinned_easing_samples(include_str!("sine_easing_reference.csv"), 99);
    }

    #[test]
    fn polynomial_easings_match_unchanged_manim_finite_samples() {
        assert_pinned_easing_samples(include_str!("polynomial_easing_reference.csv"), 420);
    }

    #[test]
    fn slow_and_double_smooth_rates_match_pinned_values_at_both_precisions() {
        for (rate, quarter, three_quarters) in [
            (
                RateFunction::SlowInto,
                0.6614378277661477_f64,
                0.9682458365518543,
            ),
            (RateFunction::DoubleSmooth, 0.25, 0.75),
        ] {
            for (progress, expected) in [
                (-1.0, 0.0),
                (0.0, 0.0),
                (0.25, quarter),
                (0.75, three_quarters),
                (1.0, 1.0),
                (2.0, 1.0),
            ] {
                assert!((rate.evaluate_f64(progress) - expected).abs() < 1e-15);
                assert_eq!(rate.evaluate(progress as f32), expected as f32);
            }
        }
        assert_eq!(RateFunction::DoubleSmooth.evaluate_f64(0.5), 0.5);
        assert!((RateFunction::SlowInto.evaluate_f64(0.5) - 0.8660254037844386).abs() < 1e-15);
    }

    #[test]
    fn rate_functions_clamp_normalized_input() {
        assert_eq!(RateFunction::Linear.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Linear.evaluate(2.0), 1.0);
        assert_eq!(RateFunction::Smooth.evaluate(-1.0), 0.0);
        assert_eq!(RateFunction::Smooth.evaluate(2.0), 1.0);
    }

    #[test]
    fn curved_vector_tracks_are_position_only_and_require_finite_arcs() {
        let mut track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Position,
            values: TrackValues::ArcVec2 {
                from: Vec2::ZERO,
                to: Vec2::new(2.0, 0.0),
                arc_angle: std::f64::consts::PI,
            },
            timing: timing(),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(validate_track_definition(&track), Ok(()));

        track.property = Property::Scale;
        assert_eq!(
            validate_track_definition(&track),
            Err(TimelineError::ArcVec2PropertyMismatch(Property::Scale))
        );

        track.property = Property::Position;
        track.values = TrackValues::ArcVec2 {
            from: Vec2::ZERO,
            to: Vec2::new(2.0, 0.0),
            arc_angle: f64::NAN,
        };
        assert!(matches!(
            validate_track_definition(&track),
            Err(TimelineError::InvalidPathArcValue { value, .. }) if value.is_nan()
        ));
    }

    #[test]
    fn mapped_presence_resolves_to_its_nested_monotone_boundary() {
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(4.0, 6.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![
                CompositionTimeMapStep::new(0.25, 0.5, RateFunction::Linear),
                CompositionTimeMapStep::new(0.5, 0.5, RateFunction::Linear),
            ]),
        };
        assert_eq!(resolve_track_timing(&track), Ok(TrackTiming::instant(7.0)));

        let mut zero_root = track.clone();
        zero_root.timing = TrackTiming::new(2.0, 0.0, RateFunction::Linear);
        assert_eq!(
            resolve_track_timing(&zero_root),
            Ok(TrackTiming::instant(2.0))
        );
    }

    #[test]
    fn mapped_presence_rejects_reversing_rate_during_validation() {
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Presence,
            values: TrackValues::Bool {
                from: false,
                to: true,
            },
            timing: TrackTiming::new(0.0, 2.0, RateFunction::Linear),
            time_map: CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
                0.0,
                1.0,
                RateFunction::ThereAndBack,
            )]),
        };
        let error = validate_track_definition(&track)
            .expect_err("a reversing parent has no single presence boundary");
        assert!(matches!(
            error,
            TimelineError::InvalidCompositionTimeMap(
                CompositionTimeMapError::UnsupportedDiscreteRate { .. }
            )
        ));
    }

    #[test]
    fn stroke_width_is_a_non_negative_scalar_timeline_property() {
        let valid = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(4),
            property: Property::StrokeWidth,
            values: TrackValues::Scalar { from: 1.0, to: 3.0 },
            timing: timing(),
            time_map: CompositionTimeMap::identity(),
        };
        validate_track_definition(&valid).expect("valid stroke-width track");

        let mut invalid = valid.clone();
        invalid.values = TrackValues::Scalar {
            from: 3.0,
            to: -1.0,
        };
        let error = validate_track_definition(&invalid)
            .expect_err("negative stroke width must fail typed-track validation");
        assert_eq!(
            error,
            TimelineError::InvalidStrokeWidthValues {
                from: 3.0,
                to: -1.0,
            }
        );
        validate_track_definition(&valid).expect("rejection does not mutate the typed track");
    }

    #[test]
    fn non_finite_fill_color_is_rejected_by_track_validation() {
        let invalid = crate::Color {
            red: f32::NAN,
            ..crate::Color::RED
        };
        let invalid_track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(1),
            property: Property::Fill,
            values: TrackValues::Color {
                from: Some(crate::Color::BLUE),
                to: Some(invalid),
            },
            timing: timing(),
            time_map: CompositionTimeMap::identity(),
        };
        assert_eq!(
            validate_track_definition(&invalid_track),
            Err(TimelineError::InvalidColorValue {
                property: Property::Fill,
                endpoint: TrackValueEndpoint::To,
            })
        );

        let valid_track = TrackDefinition {
            values: TrackValues::Color {
                from: None,
                to: Some(crate::Color::RED),
            },
            ..invalid_track
        };
        assert!(validate_track_definition(&valid_track).is_ok());
    }

    fn track(property: Property, values: TrackValues, timing: TrackTiming) -> TrackDefinition {
        TrackDefinition {
            id: TrackId::new(7),
            object: ObjectId::new(3),
            property,
            values,
            timing,
            time_map: CompositionTimeMap::identity(),
        }
    }

    #[test]
    fn typed_properties_accept_matching_values_and_reject_wrong_kinds() {
        for property in [Property::Position, Property::Scale] {
            let values = TrackValues::Vec2 {
                from: Vec2::ONE,
                to: Vec2::new(2.0, 0.5),
            };
            validate_track_definition(&track(property, values, timing())).unwrap();
            assert_eq!(
                validate_track_definition(&track(
                    property,
                    TrackValues::Scalar { from: 0.0, to: 1.0 },
                    timing()
                )),
                Err(TimelineError::ValueTypeMismatch {
                    property,
                    expected: ValueKind::Vec2,
                    actual: ValueKind::Scalar
                })
            );
        }
        for property in [
            Property::Opacity,
            Property::Rotation,
            Property::Appearance,
            Property::Reveal,
            Property::Morph,
            Property::StrokeWidth,
        ] {
            let values = TrackValues::Scalar { from: 0.0, to: 1.0 };
            validate_track_definition(&track(property, values.clone(), timing())).unwrap();
            validate_track_definition(&track(property, values, TrackTiming::instant(1.0))).unwrap();
            assert_eq!(
                validate_track_definition(&track(
                    property,
                    TrackValues::Vec2 {
                        from: Vec2::ZERO,
                        to: Vec2::ONE
                    },
                    timing()
                )),
                Err(TimelineError::ValueTypeMismatch {
                    property,
                    expected: ValueKind::Scalar,
                    actual: ValueKind::Vec2
                })
            );
        }
    }

    #[test]
    fn presence_requires_an_instant_event_without_a_composition_map() {
        let mut event = track(
            Property::Presence,
            TrackValues::Bool {
                from: false,
                to: true,
            },
            TrackTiming::instant(1.25),
        );
        assert_eq!(resolve_track_timing(&event), Ok(TrackTiming::instant(1.25)));
        event.timing.duration = 0.5;
        assert_eq!(
            validate_track_definition(&event),
            Err(TimelineError::InvalidInstantDuration {
                property: Property::Presence,
                duration: 0.5,
            })
        );
    }

    #[test]
    fn continuous_composition_map_is_validated_without_changing_track_identity() {
        let mut value = track(
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing(),
        );
        value.time_map = CompositionTimeMap::from_steps(vec![CompositionTimeMapStep::new(
            0.25,
            0.5,
            RateFunction::Smooth,
        )]);
        let before = value.clone();
        assert_eq!(resolve_track_timing(&value), Ok(timing()));
        assert_eq!(value, before);
        value.timing = TrackTiming::instant(1.0);
        assert_eq!(
            validate_track_definition(&value),
            Err(TimelineError::InstantTrackCannotUseTimeMap(
                Property::Position
            ))
        );
    }

    #[test]
    fn invalid_timing_is_rejected() {
        let mut value = track(
            Property::Position,
            TrackValues::Vec2 {
                from: Vec2::ZERO,
                to: Vec2::ONE,
            },
            timing(),
        );
        for duration in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            value.timing.duration = duration;
            assert!(matches!(
                validate_track_definition(&value),
                Err(TimelineError::InvalidDuration(_))
            ));
        }
        value.timing.duration = 1.0;
        for start in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            value.timing.start_time = start;
            assert!(matches!(
                validate_track_definition(&value),
                Err(TimelineError::InvalidStartTime(_))
            ));
        }
    }

    #[test]
    fn non_finite_scalar_and_vector_endpoints_are_rejected() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (from, to) in [(invalid, 1.0), (0.0, invalid)] {
                assert!(matches!(
                    validate_track_definition(&track(
                        Property::Opacity,
                        TrackValues::Scalar { from, to },
                        timing()
                    )),
                    Err(TimelineError::InvalidScalarValues {
                        property: Property::Opacity,
                        ..
                    })
                ));
            }
            for invalid in [Vec2::new(invalid, 0.0), Vec2::new(0.0, invalid)] {
                for (from, to) in [(invalid, Vec2::ONE), (Vec2::ZERO, invalid)] {
                    assert!(matches!(
                        validate_track_definition(&track(
                            Property::Position,
                            TrackValues::Vec2 { from, to },
                            timing()
                        )),
                        Err(TimelineError::InvalidVec2Values {
                            property: Property::Position,
                            ..
                        })
                    ));
                }
            }
        }
    }

    #[test]
    fn prepared_morph_rejects_non_finite_and_overflowing_translation_frames() {
        let validate = |frame| {
            validate_track_definition(&track(
                Property::Morph,
                TrackValues::PreparedMorph {
                    from: 0.0,
                    to: 1.0,
                    geometry: GeometryRef::path(
                        crate::VectorPath::new()
                            .move_to(Vec2::ZERO)
                            .line_to(Vec2::ONE)
                            .with_morph_target(
                                crate::VectorPath::new()
                                    .move_to(Vec2::ONE)
                                    .line_to(Vec2::ZERO),
                            ),
                    ),
                    render_frame: Some(frame),
                    source_transform: Transform2D::IDENTITY,
                },
                timing(),
            ))
        };
        assert_eq!(
            validate(MorphRenderFrame::fixed(Transform2D::IDENTITY)),
            Ok(())
        );
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for translation in [Vec2::new(invalid, 0.0), Vec2::new(0.0, invalid)] {
                assert_eq!(
                    validate(MorphRenderFrame {
                        from: Transform2D::IDENTITY,
                        to_translation: translation,
                    }),
                    Err(TimelineError::InvalidObjectValue {
                        property: Property::Morph,
                        endpoint: TrackValueEndpoint::To,
                        field: ObjectStateField::Transform,
                    })
                );
            }
        }
        // Finite endpoints alone cannot guarantee that translation sampling is finite.
        assert_eq!(
            validate(MorphRenderFrame {
                from: Transform2D {
                    translation: Vec2::new(-f32::MAX, 0.0),
                    ..Transform2D::IDENTITY
                },
                to_translation: Vec2::new(f32::MAX, 0.0),
            }),
            Err(TimelineError::InvalidObjectValue {
                property: Property::Morph,
                endpoint: TrackValueEndpoint::To,
                field: ObjectStateField::Transform,
            })
        );
    }

    #[test]
    fn path_motion_rejects_overflowing_transformed_cubic_hulls_and_accepts_zero_metric() {
        let large_path =
            std::sync::Arc::new(crate::VectorPath::new().move_to(Vec2::ZERO).cubic_to(
                Vec2::new(f32::MAX, 0.0),
                Vec2::new(0.0, f32::MAX),
                Vec2::ONE,
            ));
        let invalid = track(
            Property::Position,
            TrackValues::PathVec2 {
                path: large_path,
                path_transform: Transform2D {
                    scale: Vec2::new(2.0, 1.0),
                    ..Transform2D::IDENTITY
                },
                target_center_offset: Vec2::ZERO,
            },
            timing(),
        );
        assert_eq!(
            validate_track_definition(&invalid),
            Err(TimelineError::InvalidPathMotionValues(Property::Position))
        );

        // Zero scale is valid and gives the plan a deterministic all-zero metric.
        let zero_metric = track(
            Property::Position,
            TrackValues::PathVec2 {
                path: std::sync::Arc::new(
                    crate::VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(4.0, 0.0)),
                ),
                path_transform: Transform2D {
                    translation: Vec2::new(2.0, -1.0),
                    scale: Vec2::ZERO,
                    ..Transform2D::IDENTITY
                },
                target_center_offset: Vec2::ZERO,
            },
            timing(),
        );
        validate_track_definition(&zero_metric).unwrap();
    }

    #[test]
    fn transform_endpoints_validate_geometry_transform_and_style_on_both_sides() {
        let valid = TransformTrackEndpoint {
            geometry: GeometryRef::circle(1.0),
            transform: Transform2D::IDENTITY,
            style: Style::default(),
        };
        let mut bad_geometry = valid.clone();
        bad_geometry.geometry = GeometryRef::circle(f32::NAN);
        let mut bad_transform = valid.clone();
        bad_transform.transform.rotation = f32::INFINITY;
        let mut bad_style = valid.clone();
        bad_style.style.opacity = f32::NAN;
        for (field, invalid) in [
            (ObjectStateField::Geometry, bad_geometry),
            (ObjectStateField::Transform, bad_transform),
            (ObjectStateField::Style, bad_style),
        ] {
            for (endpoint, from, to) in [
                (TrackValueEndpoint::From, invalid.clone(), valid.clone()),
                (TrackValueEndpoint::To, valid.clone(), invalid),
            ] {
                assert_eq!(
                    validate_track_definition(&track(
                        Property::Transform,
                        TrackValues::Object { from, to },
                        timing()
                    )),
                    Err(TimelineError::InvalidObjectValue {
                        property: Property::Transform,
                        endpoint,
                        field
                    })
                );
            }
        }
        validate_track_definition(&track(
            Property::Transform,
            TrackValues::Object {
                from: valid.clone(),
                to: valid,
            },
            timing(),
        ))
        .unwrap();
    }
}
