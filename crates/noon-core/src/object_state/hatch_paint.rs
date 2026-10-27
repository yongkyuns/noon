//! Checked periodic line paint in object-local scene units; no clock or resources.
use crate::Color;

/// Creator-facing parameters. [`HatchPaint::new`] validates the complete value.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HatchPaintOptions {
    /// Perpendicular distance between line centers, in object-local scene units.
    pub spacing: f64,
    /// Line width in object-local scene units, in `[0, spacing]`.
    pub width: f64,
    /// Line direction, counterclockwise radians from the local positive X axis.
    pub angle: f64,
    /// Unwrapped periodic offsets in cells; the second axis belongs to crosshatching.
    pub phase: [f64; 2],
    /// Union the first line family with a perpendicular second family.
    pub cross: bool,
}

impl Default for HatchPaintOptions {
    fn default() -> Self {
        Self {
            spacing: 0.25,
            width: 0.04,
            angle: std::f64::consts::FRAC_PI_4,
            phase: [0.0; 2],
            cross: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HatchPaintError {
    InvalidColor,
    InvalidSpacing,
    InvalidWidth,
    InvalidAngle,
    InvalidPhase,
    InvalidSamplePoint,
    InvalidFootprint,
    InvalidProgress,
    DifferentTopology,
}

impl std::fmt::Display for HatchPaintError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidColor => "hatch colors require finite RGBA channels in [0, 1]",
            Self::InvalidSpacing => "hatch spacing must be finite and positive",
            Self::InvalidWidth => "hatch width must be finite and in [0, spacing]",
            Self::InvalidAngle => "hatch angle must be finite counterclockwise radians",
            Self::InvalidPhase => "hatch phase requires two finite cell coordinates",
            Self::InvalidSamplePoint => "hatch local sample coordinates must be finite and representable",
            Self::InvalidFootprint => "hatch filter widths require finite nonnegative cell lengths",
            Self::InvalidProgress => "hatch progress must be finite and in [0, 1]",
            Self::DifferentTopology => "hatch animation requires matching cross settings; topology changes are immediate edits",
        })
    }
}
impl std::error::Error for HatchPaintError {}

/// Immutable stripes or crosshatching around the object's local origin.
///
/// Spacing and width are scene lengths before the object's pose; resizing the
/// geometry does not change them, while object scaling stretches the paint.
/// Positive phase advances the line coordinates in periods. Integral phase
/// values repeat exactly. Static values have no playback-time dependency.
///
/// Colors interpolate in premultiplied encoded sRGB; samples exclude geometry
/// coverage, fill opacity and final object opacity. Crosshatching is the union
/// of two perpendicular line families, not two alpha-composited paint layers.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "HatchPaintInput")]
pub struct HatchPaint {
    foreground: Color,
    background: Color,
    options: HatchPaintOptions,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HatchPaintInput {
    foreground: Color,
    background: Color,
    options: HatchPaintOptions,
}

impl TryFrom<HatchPaintInput> for HatchPaint {
    type Error = HatchPaintError;
    fn try_from(value: HatchPaintInput) -> Result<Self, Self::Error> {
        Self::new(value.foreground, value.background, value.options)
    }
}

impl HatchPaint {
    pub fn new(
        foreground: Color,
        background: Color,
        options: HatchPaintOptions,
    ) -> Result<Self, HatchPaintError> {
        if [foreground, background].into_iter().any(|color| {
            [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        }) {
            return Err(HatchPaintError::InvalidColor);
        }
        if !options.spacing.is_finite() || options.spacing <= 0.0 {
            return Err(HatchPaintError::InvalidSpacing);
        }
        if !options.width.is_finite() || !(0.0..=options.spacing).contains(&options.width) {
            return Err(HatchPaintError::InvalidWidth);
        }
        if !options.angle.is_finite() {
            return Err(HatchPaintError::InvalidAngle);
        }
        if !options.phase.into_iter().all(f64::is_finite) {
            return Err(HatchPaintError::InvalidPhase);
        }
        Ok(Self {
            foreground,
            background,
            options,
        })
    }

    pub const fn options(&self) -> HatchPaintOptions {
        self.options
    }
    pub const fn spacing(&self) -> f64 {
        self.options.spacing
    }
    pub const fn width(&self) -> f64 {
        self.options.width
    }
    pub const fn angle(&self) -> f64 {
        self.options.angle
    }
    pub const fn phase(&self) -> [f64; 2] {
        self.options.phase
    }
    pub const fn cross(&self) -> bool {
        self.options.cross
    }
    /// Background then foreground, so coverage zero/one selects each endpoint.
    pub const fn colors(&self) -> [Color; 2] {
        [self.background, self.foreground]
    }

    /// Late periodic projection keeps authored phase unwrapped for animation.
    pub fn phase_wrapped(&self) -> [f64; 2] {
        self.phase().map(wrap)
    }

    pub fn interpolate(&self, to: &Self, alpha: f64) -> Result<Self, HatchPaintError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(HatchPaintError::InvalidProgress);
        }
        if self.cross() != to.cross() {
            return Err(HatchPaintError::DifferentTopology);
        }
        if alpha == 0.0 {
            return Ok(self.clone());
        }
        if alpha == 1.0 {
            return Ok(to.clone());
        }
        let mix = |a: f64, b: f64| {
            if a.is_sign_positive() == b.is_sign_positive() {
                alpha.mul_add(b - a, a)
            } else {
                a * (1.0 - alpha) + b * alpha
            }
        };
        let spacing = mix(self.spacing(), to.spacing());
        // The convex interval has width <= spacing. Clamp only arithmetic
        // roundoff of those independently evaluated convex combinations.
        let width = mix(self.width(), to.width()).min(spacing);
        Self::new(
            super::interpolate_premultiplied_color(self.foreground, to.foreground, alpha),
            super::interpolate_premultiplied_color(self.background, to.background, alpha),
            HatchPaintOptions {
                spacing,
                width,
                angle: mix(self.angle(), to.angle()),
                phase: std::array::from_fn(|i| mix(self.phase()[i], to.phase()[i])),
                cross: self.cross(),
            },
        )
    }

    pub fn sample_premultiplied(&self, local: [f64; 2]) -> Result<[f64; 4], HatchPaintError> {
        self.sample_filtered_premultiplied(local, [0.0; 2])
    }

    /// Average over the footprint's axis-aligned box in stripe coordinates.
    ///
    /// Footprint lengths are in cells after the physical pixel has been mapped
    /// through inverse object pose and the two line normals. Each axis uses
    /// exact periodic-band overlap below 256 cells and the exact duty mean at
    /// larger widths. This bounded box profile is not an exact rotated-pixel
    /// integral. Zero footprint uses half-open point membership.
    pub fn sample_filtered_premultiplied(
        &self,
        local: [f64; 2],
        footprint: [f64; 2],
    ) -> Result<[f64; 4], HatchPaintError> {
        if !local.into_iter().all(f64::is_finite) {
            return Err(HatchPaintError::InvalidSamplePoint);
        }
        if footprint.into_iter().any(|v| !v.is_finite() || v < 0.0) {
            return Err(HatchPaintError::InvalidFootprint);
        }
        let (s, c) = self.angle().sin_cos();
        let coordinates = [
            (-s * local[0] + c * local[1]) / self.spacing(),
            (c * local[0] + s * local[1]) / self.spacing(),
        ];
        if !coordinates.into_iter().all(f64::is_finite) {
            return Err(HatchPaintError::InvalidSamplePoint);
        }
        let duty = self.width() / self.spacing();
        let phase = self.phase_wrapped();
        let coverage: [f64; 2] = std::array::from_fn(|i| {
            band_coverage(wrap(coordinates[i]) + phase[i], footprint[i], duty)
        });
        let amount = if self.cross() {
            coverage[0] + coverage[1] * (1.0 - coverage[0])
        } else {
            coverage[0]
        };
        let colors = self.colors().map(|color| {
            let alpha = f64::from(color.alpha);
            [
                f64::from(color.red) * alpha,
                f64::from(color.green) * alpha,
                f64::from(color.blue) * alpha,
                alpha,
            ]
        });
        Ok(std::array::from_fn(|i| {
            colors[0][i] * (1.0 - amount) + colors[1][i] * amount
        }))
    }
}

fn wrap(value: f64) -> f64 {
    let value = value.rem_euclid(1.0);
    // Tiny negative values may round to the period itself.
    if value == 1.0 {
        0.0
    } else {
        value
    }
}

fn band_coverage(center: f64, width: f64, duty: f64) -> f64 {
    if duty == 0.0 || duty == 1.0 || width >= 256.0 {
        return duty;
    }
    let center = wrap(center);
    if width == 0.0 {
        return if center < duty * 0.5 || center >= 1.0 - duty * 0.5 {
            1.0
        } else {
            0.0
        };
    }
    let periods = width.floor();
    let remainder = width - periods;
    if remainder == 0.0 {
        return duty;
    }
    // Every whole period has exact area `duty`. The remainder is centered at
    // center +/- periods/2; only the parity changes the periodic coordinate.
    let residual_center = center - 0.5 * ((periods as u32) & 1) as f64;
    let residual_center = wrap(residual_center + 0.5) - 0.5;
    let mut residual = 0.0;
    for integer in [-1.0, 0.0, 1.0] {
        // Compare distances in footprint units. Forming two absolute interval
        // endpoints first would erase tiny widths near a nonzero coordinate.
        let high = ((integer + duty * 0.5 - residual_center) / remainder + 0.5).clamp(0.0, 1.0);
        let low = ((integer - duty * 0.5 - residual_center) / remainder + 0.5).clamp(0.0, 1.0);
        residual += high - low;
    }
    ((periods * duty + remainder * residual) / width).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests;
