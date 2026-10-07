//! Typed, renderer-independent inputs for the first glow treatment.
//!
//! These are inert values, not attachments or a second effects engine. Scene
//! publication, generational identity, channel ownership and animation timing
//! remain with their existing owners. No public scene setter is enabled before
//! lowering and rendering can support it. See #1897 and tests/visual-effects.

use crate::Color;

/// A radius explicitly measured in final output pixels, not CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pixels(pub f64);

/// Gaussian sigma in world scene units or final output pixels.
///
/// Object scale changes source geometry, not this radius. A plain f64 converts
/// to Scene; values are validated when an update is applied or prepared.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlowRadius {
    Scene(f64),
    Pixels(f64),
}

impl From<f64> for GlowRadius {
    fn from(value: f64) -> Self {
        Self::Scene(value)
    }
}

impl From<Pixels> for GlowRadius {
    fn from(value: Pixels) -> Self {
        Self::Pixels(value.0)
    }
}

impl GlowRadius {
    pub const fn value(self) -> f64 {
        match self {
            Self::Scene(value) | Self::Pixels(value) => value,
        }
    }

    fn validate(self) -> Result<(), GlowParameterError> {
        nonnegative(self.value(), GlowParameterError::InvalidRadius)
    }

    fn same_unit(self, other: Self) -> bool {
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
    ) -> Result<f64, GlowParameterError> {
        self.validate()?;
        if output_height == 0 || !world_view_height.is_finite() || world_view_height <= 0.0 {
            return Err(GlowParameterError::InvalidView);
        }
        let pixels = match self {
            Self::Scene(value) => (value / world_view_height) * f64::from(output_height),
            Self::Pixels(value) => value,
        };
        nonnegative(pixels, GlowParameterError::InvalidRadius)?;
        Ok(pixels)
    }

    fn interpolate(self, target: Self, alpha: f64) -> Self {
        let value = interpolate(self.value(), target.value(), alpha);
        match self {
            Self::Scene(_) => Self::Scene(value),
            Self::Pixels(_) => Self::Pixels(value),
        }
    }
}

/// What supplies the halo mask; source mode is not an animatable scalar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GlowSource {
    #[default]
    Painted,
    Silhouette,
}

/// Parameters addressable by this finite schema. Attachment identity is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlowParameter {
    Color,
    Radius,
    Intensity,
    Source,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlowParameterError {
    InvalidColor,
    InvalidRadius,
    InvalidIntensity,
    InvalidProgress,
    InvalidView,
    DiscreteTransition(GlowParameter),
}

impl std::fmt::Display for GlowParameterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidColor => {
                f.write_str("glow color must have finite RGBA channels in [0, 1]")
            }
            Self::InvalidRadius => f.write_str("glow radius must be finite and non-negative"),
            Self::InvalidIntensity => f.write_str("glow intensity must be finite and in [0, 8]"),
            Self::InvalidProgress => f.write_str("glow progress must be finite and in [0, 1]"),
            Self::InvalidView => f.write_str("glow requires a positive finite uniform 2D view"),
            Self::DiscreteTransition(parameter) => {
                write!(
                    f,
                    "glow {parameter:?} cannot change discretely during interpolation"
                )
            }
        }
    }
}

impl std::error::Error for GlowParameterError {}

/// Complete validated glow definition. Copying it allocates no resources.
///
/// This is not an effect handle or a mutable copy of an attached object's state.
/// Fields are private so invalid intermediate values cannot become a definition.
///
/// ```
/// use noon_core::{Glow, GlowParameterError, GlowUpdate, Pixels};
/// # fn main() -> Result<(), GlowParameterError> {
/// let halo = Glow::new(GlowUpdate::default().radius(Pixels(12.0)).intensity(0.25))?;
/// let brighter = GlowUpdate::default().intensity(1.4).apply_to(halo)?;
/// assert_eq!(brighter.radius(), halo.radius());
/// assert_eq!(halo.intensity(), 0.25);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    color: Color,
    radius: GlowRadius,
    intensity: f64,
    source: GlowSource,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            radius: GlowRadius::Scene(0.15),
            intensity: 0.35,
            source: GlowSource::Painted,
        }
    }
}

impl Glow {
    /// Construct from documented defaults and one checked partial update.
    pub fn new(update: GlowUpdate) -> Result<Self, GlowParameterError> {
        update.apply_to(Self::default())
    }

    pub const fn color(self) -> Color {
        self.color
    }

    pub const fn radius(self) -> GlowRadius {
        self.radius
    }

    pub const fn intensity(self) -> f64 {
        self.intensity
    }

    pub const fn source(self) -> GlowSource {
        self.source
    }

    /// Neutral for the specified artistic LDR halo; not an HDR-emission rule.
    pub fn is_neutral(self) -> bool {
        self.intensity == 0.0 || self.radius.value() == 0.0 || self.color.alpha == 0.0
    }
}

/// A partial authored request. Default is empty, NOT the Glow defaults.
///
/// Setters compose on the receiver/target in the public API; this value crosses
/// the shared Rust boundary. None means preserve the current parameter.
///
/// ```compile_fail,E0308
/// use noon_core::GlowUpdate;
/// let _ = GlowUpdate::default().intensity(true);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlowUpdate {
    pub color: Option<Color>,
    pub radius: Option<GlowRadius>,
    pub intensity: Option<f64>,
    pub source: Option<GlowSource>,
}

impl GlowUpdate {
    pub fn color(mut self, value: Color) -> Self {
        self.color = Some(value);
        self
    }

    pub fn radius(mut self, value: impl Into<GlowRadius>) -> Self {
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

    pub const fn writes(self, parameter: GlowParameter) -> bool {
        match parameter {
            GlowParameter::Color => self.color.is_some(),
            GlowParameter::Radius => self.radius.is_some(),
            GlowParameter::Intensity => self.intensity.is_some(),
            GlowParameter::Source => self.source.is_some(),
        }
    }

    /// Validate the entire request before returning a replacement value. No
    /// mutation or rollback copy lives in a language wrapper.
    pub fn apply_to(self, current: Glow) -> Result<Glow, GlowParameterError> {
        if let Some(color) = self.color {
            if ![color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            {
                return Err(GlowParameterError::InvalidColor);
            }
        }
        if let Some(radius) = self.radius {
            radius.validate()?;
        }
        if let Some(intensity) = self.intensity {
            if !intensity.is_finite() || !(0.0..=8.0).contains(&intensity) {
                return Err(GlowParameterError::InvalidIntensity);
            }
        }
        Ok(Glow {
            color: self.color.unwrap_or(current.color),
            radius: self.radius.unwrap_or(current.radius),
            intensity: self.intensity.unwrap_or(current.intensity),
            source: self.source.unwrap_or(current.source),
        })
    }

    /// Prepare interpolation from an explicitly supplied activation-time value.
    ///
    /// Runtime supplies that coherent value and owns admission/leases. This
    /// operation neither captures from a store nor starts a timeline. A radius
    /// unit or source change fails here, even for a zero-strength treatment.
    pub fn prepare(self, captured: Glow) -> Result<PreparedGlowUpdate, GlowParameterError> {
        let target = self.apply_to(captured)?;
        if !captured.radius.same_unit(target.radius) {
            return Err(GlowParameterError::DiscreteTransition(GlowParameter::Radius));
        }
        if captured.source != target.source {
            return Err(GlowParameterError::DiscreteTransition(GlowParameter::Source));
        }
        Ok(PreparedGlowUpdate {
            captured,
            target,
            request: self,
        })
    }
}

/// Immutable parameter interpolation, not animation intent, a driver or a clock.
///
/// It consumes already-mapped progress from the existing timing machinery and
/// emits only requested parameters. Unmentioned current values are never copied
/// back, so an intensity update cannot restore an old radius or color. Runtime
/// must still filter writes through actual attachment generation/channel leases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedGlowUpdate {
    captured: Glow,
    target: Glow,
    request: GlowUpdate,
}

impl PreparedGlowUpdate {
    pub const fn writes(self, parameter: GlowParameter) -> bool {
        self.request.writes(parameter)
    }

    pub fn sample(self, alpha: f64) -> Result<GlowUpdate, GlowParameterError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(GlowParameterError::InvalidProgress);
        }
        let color = |from: f32, to: f32| interpolate(f64::from(from), f64::from(to), alpha) as f32;
        let update = GlowUpdate {
            color: self.request.color.map(|_| {
                Color::rgba(
                    color(self.captured.color.red, self.target.color.red),
                    color(self.captured.color.green, self.target.color.green),
                    color(self.captured.color.blue, self.target.color.blue),
                    color(self.captured.color.alpha, self.target.color.alpha),
                )
            }),
            radius: self
                .request
                .radius
                .map(|_| self.captured.radius.interpolate(self.target.radius, alpha)),
            intensity: self
                .request
                .intensity
                .map(|_| interpolate(self.captured.intensity, self.target.intensity, alpha)),
            source: self.request.source,
        };
        // Keep even extreme finite radius arithmetic fail-closed.
        update.apply_to(self.captured)?;
        Ok(update)
    }
}

fn nonnegative(value: f64, error: GlowParameterError) -> Result<(), GlowParameterError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(error)
    }
}

fn interpolate(from: f64, to: f64, alpha: f64) -> f64 {
    if alpha == 0.0 {
        from
    } else if alpha == 1.0 {
        to
    } else {
        from * (1.0 - alpha) + to * alpha
    }
}

#[cfg(test)]
mod tests;
