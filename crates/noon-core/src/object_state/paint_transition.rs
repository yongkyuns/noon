//! Checked immutable endpoints for the ordinary Fill channel.
use std::sync::Arc;

use crate::{
    Color, GradientStop, ImagePaint, ImagePaintError, LinearGradient, LinearGradientError,
    SemanticPaint,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintTransitionError {
    UnsupportedPaint,
    InvalidOpacity,
    InvalidProgress,
    DifferentStopCounts,
    DegenerateAxis,
    Gradient(LinearGradientError),
    Image(ImagePaintError),
}

impl std::fmt::Display for PaintTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPaint => f.write_str("fill transitions require a gradient with compatible paint, or two image paints using the same resource, sampling and mapping topology; image-to-solid transitions are not supported"),
            Self::InvalidOpacity => f.write_str("paint transition opacity must be finite and in [0, 1]"),
            Self::InvalidProgress => f.write_str("paint transition progress must be finite and in [0, 1]"),
            Self::DifferentStopCounts => f.write_str("animated gradients require matching stop counts"),
            Self::DegenerateAxis => f.write_str("gradient axis would collapse or lose coordinate precision during interpolation"),
            Self::Gradient(error) => error.fmt(f),
            Self::Image(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for PaintTransitionError {}
impl From<LinearGradientError> for PaintTransitionError {
    fn from(error: LinearGradientError) -> Self {
        Self::Gradient(error)
    }
}
impl From<ImagePaintError> for PaintTransitionError {
    fn from(error: ImagePaintError) -> Self {
        Self::Image(error)
    }
}

/// Prepared value endpoints, with no scene identity, scheduling or mutable state.
///
/// Solids and absent paint promote to the gradient's stop topology for interior
/// samples. Exact endpoint samples preserve the original paint kind and f64
/// opacity. Colors interpolate in premultiplied encoded-sRGB; paint opacity
/// interpolates independently. Changing stop count is an immediate value edit,
/// not an interpolated topology change.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "PaintTransitionInput")]
pub struct PaintTransition {
    from: Option<SemanticPaint>,
    from_opacity: f64,
    to: Option<SemanticPaint>,
    to_opacity: f64,
    #[serde(skip)]
    values: PaintTransitionValues,
}

#[derive(Clone, Debug, PartialEq)]
enum PaintTransitionValues {
    Gradients([Arc<LinearGradient>; 2]),
    Images([ImagePaint; 2]),
}

#[derive(serde::Deserialize)]
struct PaintTransitionInput {
    from: Option<SemanticPaint>,
    from_opacity: f64,
    to: Option<SemanticPaint>,
    to_opacity: f64,
}
impl TryFrom<PaintTransitionInput> for PaintTransition {
    type Error = PaintTransitionError;
    fn try_from(input: PaintTransitionInput) -> Result<Self, Self::Error> {
        Self::new(input.from, input.from_opacity, input.to, input.to_opacity)
    }
}

impl PaintTransition {
    pub fn new(
        from: Option<SemanticPaint>,
        from_opacity: f64,
        to: Option<SemanticPaint>,
        to_opacity: f64,
    ) -> Result<Self, PaintTransitionError> {
        if [from_opacity, to_opacity]
            .into_iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(&v))
        {
            return Err(PaintTransitionError::InvalidOpacity);
        }
        if let (Some(SemanticPaint::Image(a)), Some(SemanticPaint::Image(b))) = (&from, &to) {
            // Validation covers the complete scale interval: same signed,
            // nonzero scales and matching immutable resource/topology.
            a.interpolate(**b, 0.5)?;
            let values = PaintTransitionValues::Images([**a, **b]);
            return Ok(Self {
                from,
                from_opacity,
                to,
                to_opacity,
                values,
            });
        }
        let gradient = |paint: &Option<SemanticPaint>| match paint {
            Some(SemanticPaint::LinearGradient(gradient)) => Some(gradient.clone()),
            _ => None,
        };
        let template = gradient(&from)
            .or_else(|| gradient(&to))
            .ok_or(PaintTransitionError::UnsupportedPaint)?;
        let promote =
            |paint: &Option<SemanticPaint>| -> Result<Arc<LinearGradient>, PaintTransitionError> {
                match paint {
                    Some(SemanticPaint::LinearGradient(gradient)) => Ok(gradient.clone()),
                    Some(SemanticPaint::Resource(_) | SemanticPaint::Image(_)) => {
                        Err(PaintTransitionError::UnsupportedPaint)
                    }
                    Some(SemanticPaint::Solid(_)) | None => {
                        let color = match paint {
                            Some(SemanticPaint::Solid(color)) => *color,
                            _ => Color::rgba(0.0, 0.0, 0.0, 0.0),
                        };
                        let stops = template
                            .stops()
                            .iter()
                            .map(|stop| GradientStop::new(stop.position(), color))
                            .collect::<Result<Vec<_>, _>>()?;
                        Ok(Arc::new(LinearGradient::new(
                            template.start(),
                            template.end(),
                            stops,
                        )?))
                    }
                }
            };
        let gradients = [promote(&from)?, promote(&to)?];
        if gradients[0].stops().len() != gradients[1].stops().len() {
            return Err(PaintTransitionError::DifferentStopCounts);
        }
        validate_axis_sweep(&gradients[0], &gradients[1])?;
        Ok(Self {
            from,
            from_opacity,
            to,
            to_opacity,
            values: PaintTransitionValues::Gradients(gradients),
        })
    }

    pub fn gradients(&self) -> Option<&[Arc<LinearGradient>; 2]> {
        match &self.values {
            PaintTransitionValues::Gradients(values) => Some(values),
            _ => None,
        }
    }

    pub fn images(&self) -> Option<&[ImagePaint; 2]> {
        match &self.values {
            PaintTransitionValues::Images(values) => Some(values),
            _ => None,
        }
    }

    pub(crate) fn remap_image_resources(
        &mut self,
        mut remap: impl FnMut(crate::RasterImageResourceHandle) -> crate::RasterImageResourceHandle,
    ) {
        if let PaintTransitionValues::Images(images) = &mut self.values {
            for image in images {
                *image =
                    ImagePaint::new(remap(image.resource()), image.mapping(), image.sampling());
            }
            for paint in [&mut self.from, &mut self.to] {
                if let Some(SemanticPaint::Image(image)) = paint {
                    *image = Arc::new(ImagePaint::new(
                        remap(image.resource()),
                        image.mapping(),
                        image.sampling(),
                    ));
                }
            }
        }
    }

    /// Analytic minimum axis length over the complete interpolation interval.
    pub fn minimum_axis_length(&self) -> f64 {
        self.gradients().map_or(f64::INFINITY, |values| {
            minimum_axis_length(&values[0], &values[1])
        })
    }

    pub fn endpoints(&self) -> [(Option<&SemanticPaint>, f64); 2] {
        [
            (self.from.as_ref(), self.from_opacity),
            (self.to.as_ref(), self.to_opacity),
        ]
    }

    pub fn sample(&self, alpha: f64) -> Result<(Option<SemanticPaint>, f64), PaintTransitionError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(PaintTransitionError::InvalidProgress);
        }
        if alpha == 0.0 {
            return Ok((self.from.clone(), self.from_opacity));
        }
        if alpha == 1.0 {
            return Ok((self.to.clone(), self.to_opacity));
        }
        let opacity = mix(self.from_opacity, self.to_opacity, alpha);
        if self.from == self.to {
            return Ok((self.from.clone(), opacity));
        }
        let [from, to] = match &self.values {
            PaintTransitionValues::Images([from, to]) => {
                return Ok((
                    Some(SemanticPaint::Image(Arc::new(
                        from.interpolate(*to, alpha)?,
                    ))),
                    opacity,
                ));
            }
            PaintTransitionValues::Gradients(values) => values,
        };
        let point = |a: [f64; 2], b: [f64; 2]| [mix(a[0], b[0], alpha), mix(a[1], b[1], alpha)];
        let stops = from
            .stops()
            .iter()
            .zip(to.stops())
            .map(|(a, b)| {
                let color = |color: Color| {
                    let alpha = f64::from(color.alpha);
                    [
                        f64::from(color.red) * alpha,
                        f64::from(color.green) * alpha,
                        f64::from(color.blue) * alpha,
                        alpha,
                    ]
                };
                let a_color = color(a.color());
                let b_color = color(b.color());
                let c: [f64; 4] = std::array::from_fn(|i| mix(a_color[i], b_color[i], alpha));
                let color = if c[3] == 0.0 {
                    Color::rgba(0.0, 0.0, 0.0, 0.0)
                } else {
                    Color::rgba(
                        (c[0] / c[3]) as f32,
                        (c[1] / c[3]) as f32,
                        (c[2] / c[3]) as f32,
                        c[3] as f32,
                    )
                };
                GradientStop::new(mix(a.position(), b.position(), alpha), color)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let gradient = LinearGradient::new(
            point(from.start(), to.start()),
            point(from.end(), to.end()),
            stops,
        )?;
        Ok((
            Some(SemanticPaint::LinearGradient(Arc::new(gradient))),
            opacity,
        ))
    }
}

fn mix(from: f64, to: f64, alpha: f64) -> f64 {
    from * (1.0 - alpha) + to * alpha
}

fn validate_axis_sweep(
    from: &LinearGradient,
    to: &LinearGradient,
) -> Result<(), PaintTransitionError> {
    if from.start() == to.start() && from.end() == to.end() {
        return Ok(());
    }
    let minimum = minimum_axis_length(from, to);
    let coordinate_scale = from
        .start()
        .into_iter()
        .chain(from.end())
        .chain(to.start())
        .chain(to.end())
        .map(f64::abs)
        .fold(0.0, f64::max);
    if minimum <= 8.0 * f64::EPSILON * coordinate_scale {
        return Err(PaintTransitionError::DegenerateAxis);
    }
    Ok(())
}

fn minimum_axis_length(from: &LinearGradient, to: &LinearGradient) -> f64 {
    let axis = |gradient: &LinearGradient| {
        [
            gradient.end()[0] - gradient.start()[0],
            gradient.end()[1] - gradient.start()[1],
        ]
    };
    let a = axis(from);
    let b = axis(to);
    let scale = a.into_iter().chain(b).map(f64::abs).fold(0.0, f64::max);
    let a = a.map(|v| v / scale);
    let b = b.map(|v| v / scale);
    let delta = [b[0] - a[0], b[1] - a[1]];
    let squared = delta[0] * delta[0] + delta[1] * delta[1];
    let alpha = if squared == 0.0 {
        0.0
    } else {
        (-(a[0] * delta[0] + a[1] * delta[1]) / squared).clamp(0.0, 1.0)
    };
    mix(a[0], b[0], alpha).hypot(mix(a[1], b[1], alpha)) * scale
}

#[cfg(test)]
mod tests;
