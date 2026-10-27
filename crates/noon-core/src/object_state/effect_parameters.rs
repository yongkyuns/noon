//! Finite checked effect parameters shared by the built-in leaf filters.
use crate::{Color, EffectDefinition, GlowSource};

/// A radius explicitly measured in final output pixels, not CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pixels(pub f64);

/// Gaussian sigma in world scene units or final output pixels.
///
/// Object scale changes source geometry, not this radius. A plain f64 converts
/// to Scene; values are validated when an update is applied or prepared.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EffectRadius {
    Scene(f64),
    Pixels(f64),
}

impl From<f64> for EffectRadius {
    fn from(value: f64) -> Self {
        Self::Scene(value)
    }
}

impl From<Pixels> for EffectRadius {
    fn from(value: Pixels) -> Self {
        Self::Pixels(value.0)
    }
}

impl EffectRadius {
    pub const fn value(self) -> f64 {
        match self {
            Self::Scene(value) | Self::Pixels(value) => value,
        }
    }

    pub(super) fn validate(self) -> Result<(), EffectParameterError> {
        nonnegative(self.value(), EffectParameterError::InvalidRadius)
    }

    pub(super) fn same_unit(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Scene(_), Self::Scene(_)) | (Self::Pixels(_), Self::Pixels(_))
        )
    }

    /// Resolve a validated uniform 2D view. Projection support and device limits
    /// are checked by the caller; no backend-specific limit is a semantic unit.
    pub fn to_output_pixels(
        self,
        output_height: u32,
        world_view_height: f64,
    ) -> Result<f64, EffectParameterError> {
        self.validate()?;
        if output_height == 0 || !world_view_height.is_finite() || world_view_height <= 0.0 {
            return Err(EffectParameterError::InvalidView);
        }
        let pixels = match self {
            Self::Scene(value) => (value / world_view_height) * f64::from(output_height),
            Self::Pixels(value) => value,
        };
        nonnegative(pixels, EffectParameterError::InvalidRadius)?;
        Ok(pixels)
    }

    pub(super) fn interpolate(self, target: Self, alpha: f64) -> Self {
        let value = interpolate(self.value(), target.value(), alpha);
        match self {
            Self::Scene(_) => Self::Scene(value),
            Self::Pixels(_) => Self::Pixels(value),
        }
    }
}

/// Parameters addressable by this finite schema. Attachment identity is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectParameter {
    Color,
    Radius,
    Intensity,
    Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectParameterError {
    InvalidColor,
    InvalidRadius,
    InvalidIntensity,
    InvalidProgress,
    InvalidView,
    DiscreteTransition(EffectParameter),
    UnsupportedParameter(EffectParameter),
    IncompatibleDefinition,
}

impl std::fmt::Display for EffectParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidColor => {
                f.write_str("effect color must have finite RGBA channels in [0, 1]")
            }
            Self::InvalidRadius => f.write_str("effect radius must be finite and non-negative"),
            Self::InvalidIntensity => f.write_str("effect intensity must be finite and in [0, 8]"),
            Self::InvalidProgress => f.write_str("effect progress must be finite and in [0, 1]"),
            Self::InvalidView => f.write_str("effect requires a positive finite uniform 2D view"),
            Self::UnsupportedParameter(parameter) => write!(
                f,
                "effect parameter {parameter:?} is unsupported by this definition"
            ),
            Self::IncompatibleDefinition => {
                f.write_str("effect definition kinds cannot change during interpolation")
            }
            Self::DiscreteTransition(parameter) => {
                write!(
                    f,
                    "effect {parameter:?} cannot change discretely during interpolation"
                )
            }
        }
    }
}

impl std::error::Error for EffectParameterError {}

/// A partial authored request. Default is empty, NOT the Glow defaults.
///
/// Setters compose on the receiver/target in the public API; this value crosses
/// the shared Rust boundary. None means preserve the current parameter.
///
/// ```compile_fail,E0308
/// use noon_core::EffectUpdate;
/// let _ = EffectUpdate::default().intensity(true);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EffectUpdate {
    pub color: Option<Color>,
    pub radius: Option<EffectRadius>,
    pub intensity: Option<f64>,
    pub source: Option<GlowSource>,
}

impl EffectUpdate {
    pub fn color(mut self, value: Color) -> Self {
        self.color = Some(value);
        self
    }

    pub fn radius(mut self, value: impl Into<EffectRadius>) -> Self {
        self.radius = Some(value.into());
        self
    }

    pub fn intensity(mut self, value: f64) -> Self {
        self.intensity = Some(value);
        self
    }

    pub fn source(mut self, value: GlowSource) -> Self {
        self.source = Some(value);
        self
    }

    /// Schema-ordered explicit write channels, including unchanged values.
    /// Empty patches own no channels. This iterator allocates no storage.
    pub fn parameters(self) -> impl Iterator<Item = EffectParameter> {
        [
            EffectParameter::Color,
            EffectParameter::Radius,
            EffectParameter::Intensity,
            EffectParameter::Source,
        ]
        .into_iter()
        .filter(move |parameter| self.writes(*parameter))
    }

    pub const fn writes(self, parameter: EffectParameter) -> bool {
        match parameter {
            EffectParameter::Color => self.color.is_some(),
            EffectParameter::Radius => self.radius.is_some(),
            EffectParameter::Intensity => self.intensity.is_some(),
            EffectParameter::Source => self.source.is_some(),
        }
    }

    pub fn apply_to<T: EffectValue>(self, current: T) -> Result<T, EffectParameterError> {
        current.apply_effect_update(self)
    }

    pub fn prepare<T: EffectValue>(
        self,
        captured: T,
    ) -> Result<PreparedEffectUpdate<T>, EffectParameterError> {
        let target = self.apply_to(captured)?;
        captured.validate_effect_transition(target)?;
        Ok(PreparedEffectUpdate {
            captured,
            target,
            request: self,
        })
    }
}

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Closed value operations for the finite built-ins and their shared definition.
/// External shader schemas are a separate checked resource surface.
pub trait EffectValue: sealed::Sealed + Copy {
    #[doc(hidden)]
    fn apply_effect_update(self, update: EffectUpdate) -> Result<Self, EffectParameterError>;
    #[doc(hidden)]
    fn validate_effect_transition(self, target: Self) -> Result<(), EffectParameterError>;
    #[doc(hidden)]
    fn sample_effect_parameters(
        self,
        target: Self,
        request: EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError>;
}

/// Immutable captured endpoints. Runtime supplies progress and owns all clocks,
/// attachment identity and publication; this value samples requested fields only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedEffectUpdate<T: EffectValue = EffectDefinition> {
    captured: T,
    target: T,
    request: EffectUpdate,
}
impl<T: EffectValue> PreparedEffectUpdate<T> {
    pub const fn writes(self, parameter: EffectParameter) -> bool {
        self.request.writes(parameter)
    }
    pub fn sample(self, alpha: f64) -> Result<EffectUpdate, EffectParameterError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(EffectParameterError::InvalidProgress);
        }
        self.captured
            .sample_effect_parameters(self.target, self.request, alpha)
    }
}

pub(super) fn nonnegative(
    value: f64,
    error: EffectParameterError,
) -> Result<(), EffectParameterError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(error)
    }
}

pub(super) fn interpolate(from: f64, to: f64, alpha: f64) -> f64 {
    if alpha == 0.0 {
        from
    } else if alpha == 1.0 {
        to
    } else {
        from * (1.0 - alpha) + to * alpha
    }
}
