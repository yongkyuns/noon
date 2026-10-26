//! Inert, checked local image-paint values. Pixels stay in the existing immutable
//! raster arena. A copied handle is a lookup capability, not a resource owner.
//!
//! Local points use the same lower-left normalized geometry box as gradients.
//! Image rows are top-first. Sampling returns encoded-premultiplied RGBA before
//! geometry coverage, fill opacity or final object opacity. These values do not
//! by themselves enable a Scene setter or renderer admission profile.
//!
//! Nearest samples floor(uv * dimensions); linear uses the four neighboring
//! texel centers. Bicubic uses the existing raster image cubic-convolution
//! kernel (coefficient -1), then clamps alpha to [0,1] and RGB to [0,alpha].
//! No sampling mode owns a playback clock or admits pixels while sampling.

use crate::{
    RasterImageResource, RasterImageResourceArena, RasterImageResourceHandle, RasterImageSampling,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    Stretch,
    #[default]
    Cover,
    Contain,
}

/// Extension applies independently to each filter tap, after premultiplication.
/// Contain additionally clips its fitted rectangle, preserving empty margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageAddressMode {
    Transparent,
    #[default]
    Clamp,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImagePaintError {
    InvalidAlignment,
    InvalidTransform,
    InvalidBounds,
    InvalidSamplePoint,
    InvalidProgress,
    DifferentMappingTopology,
    SingularInterpolation,
    DifferentResourceOrSampling,
    MissingResource,
}

impl std::fmt::Display for ImagePaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidAlignment => "image alignment must be finite and in [0, 1]",
            Self::InvalidTransform => "image UV transform requires finite translation/rotation and finite nonzero scales",
            Self::InvalidBounds => "image fitting requires positive finite local dimensions and representable fit ratios",
            Self::InvalidSamplePoint => "image sample coordinates must be finite and representable",
            Self::InvalidProgress => "image mapping progress must be finite and in [0, 1]",
            Self::DifferentMappingTopology => "image mapping interpolation requires matching fit and address modes",
            Self::SingularInterpolation => "image UV scales cannot cross zero during interpolation",
            Self::DifferentResourceOrSampling => "image paint interpolation requires the same immutable resource and sampling mode",
            Self::MissingResource => "image paint resource is stale, foreign or retired",
        })
    }
}
impl std::error::Error for ImagePaintError {}

/// Image placement about the center of the normalized local box. Positive
/// rotation turns counterclockwise; negative scales mirror the image. Placement
/// transforms the fitted image, and sampling applies the inverse transform.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "ImageUvTransformInput")]
pub struct ImageUvTransform {
    translation: [f64; 2],
    scale: [f64; 2],
    rotation: f64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageUvTransformInput {
    translation: [f64; 2],
    scale: [f64; 2],
    rotation: f64,
}

impl TryFrom<ImageUvTransformInput> for ImageUvTransform {
    type Error = ImagePaintError;
    fn try_from(value: ImageUvTransformInput) -> Result<Self, Self::Error> {
        Self::new(value.translation, value.scale, value.rotation)
    }
}

impl Default for ImageUvTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 2],
            scale: [1.0; 2],
            rotation: 0.0,
        }
    }
}

impl ImageUvTransform {
    pub fn new(
        translation: [f64; 2],
        scale: [f64; 2],
        rotation: f64,
    ) -> Result<Self, ImagePaintError> {
        if !translation
            .into_iter()
            .chain(scale)
            .chain([rotation])
            .all(f64::is_finite)
            || scale.contains(&0.0)
        {
            return Err(ImagePaintError::InvalidTransform);
        }
        Ok(Self {
            translation,
            scale,
            rotation,
        })
    }

    pub const fn translation(self) -> [f64; 2] {
        self.translation
    }
    pub const fn scale(self) -> [f64; 2] {
        self.scale
    }
    pub const fn rotation(self) -> f64 {
        self.rotation
    }

    fn inverse(self, point: [f64; 2]) -> Result<[f64; 2], ImagePaintError> {
        let delta = [
            point[0] - 0.5 - self.translation[0],
            point[1] - 0.5 - self.translation[1],
        ];
        let (s, c) = self.rotation.sin_cos();
        let result = [
            (c * delta[0] + s * delta[1]) / self.scale[0] + 0.5,
            (-s * delta[0] + c * delta[1]) / self.scale[1] + 0.5,
        ];
        if result.into_iter().all(f64::is_finite) {
            Ok(result)
        } else {
            Err(ImagePaintError::InvalidSamplePoint)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "ImageMappingInput")]
pub struct ImageMapping {
    fit: ImageFit,
    alignment: [f64; 2],
    address: ImageAddressMode,
    transform: ImageUvTransform,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageMappingInput {
    fit: ImageFit,
    alignment: [f64; 2],
    address: ImageAddressMode,
    transform: ImageUvTransform,
}
impl TryFrom<ImageMappingInput> for ImageMapping {
    type Error = ImagePaintError;
    fn try_from(value: ImageMappingInput) -> Result<Self, Self::Error> {
        Self::new(value.fit, value.alignment, value.address, value.transform)
    }
}
impl Default for ImageMapping {
    fn default() -> Self {
        Self {
            fit: ImageFit::Cover,
            alignment: [0.5; 2],
            address: ImageAddressMode::Clamp,
            transform: ImageUvTransform::default(),
        }
    }
}

impl ImageMapping {
    pub fn new(
        fit: ImageFit,
        alignment: [f64; 2],
        address: ImageAddressMode,
        transform: ImageUvTransform,
    ) -> Result<Self, ImagePaintError> {
        if !alignment
            .into_iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        {
            return Err(ImagePaintError::InvalidAlignment);
        }
        Ok(Self {
            fit,
            alignment,
            address,
            transform,
        })
    }
    pub const fn fit(self) -> ImageFit {
        self.fit
    }
    pub const fn alignment(self) -> [f64; 2] {
        self.alignment
    }
    pub const fn address(self) -> ImageAddressMode {
        self.address
    }
    pub const fn transform(self) -> ImageUvTransform {
        self.transform
    }

    /// Top-first image coordinates, or None for an empty contain margin.
    /// Bounds are untransformed geometry dimensions, so ordinary object pose
    /// never rewrites mapping or changes the intrinsic fit aspect ratio.
    pub fn image_uv(
        self,
        point: [f64; 2],
        local_size: [f64; 2],
        image_size: [u32; 2],
    ) -> Result<Option<[f64; 2]>, ImagePaintError> {
        if !point.into_iter().all(f64::is_finite) {
            return Err(ImagePaintError::InvalidSamplePoint);
        }
        if !local_size.into_iter().all(|v| v.is_finite() && v > 0.0) || image_size.contains(&0) {
            return Err(ImagePaintError::InvalidBounds);
        }
        let point = self.transform.inverse(point)?;
        if self.fit == ImageFit::Stretch {
            return Ok(Some([point[0], 1.0 - point[1]]));
        }
        // The ratio compares image and local-box aspect ratios. Check it before
        // reciprocal use; extreme but finite authored bounds may be unrenderable.
        let image_aspect = f64::from(image_size[0]) / f64::from(image_size[1]);
        let inverse_local_aspect = local_size[1] / local_size[0];
        let ratio = if inverse_local_aspect.is_infinite() {
            // A small image aspect can bring an overflowing local ratio back
            // into range. Multiply before dividing only in that case.
            (image_aspect * local_size[1]) / local_size[0]
        } else {
            image_aspect * inverse_local_aspect
        };
        if !ratio.is_finite() || ratio <= 0.0 || !ratio.recip().is_finite() {
            return Err(ImagePaintError::InvalidBounds);
        }
        let size = match self.fit {
            ImageFit::Stretch => [1.0; 2],
            ImageFit::Cover if ratio >= 1.0 => [ratio, 1.0],
            ImageFit::Cover => [1.0, ratio.recip()],
            ImageFit::Contain if ratio >= 1.0 => [1.0, ratio.recip()],
            ImageFit::Contain => [ratio, 1.0],
        };
        let uv: [f64; 2] = std::array::from_fn(|axis| {
            if size[axis] >= 1.0 {
                // Cover may have a huge extent. Avoid overflowing the offset
                // subtraction when its final normalized coordinate is finite.
                point[axis] / size[axis] + (1.0 - size[axis].recip()) * self.alignment[axis]
            } else {
                (point[axis] - (1.0 - size[axis]) * self.alignment[axis]) / size[axis]
            }
        });
        if !uv.into_iter().all(f64::is_finite) {
            return Err(ImagePaintError::InvalidSamplePoint);
        }
        if self.fit == ImageFit::Contain && uv.into_iter().any(|v| !(0.0..=1.0).contains(&v)) {
            return Ok(None);
        }
        Ok(Some([uv[0], 1.0 - uv[1]]))
    }

    /// Checked complete-interval admission. Fit/address/resource changes are
    /// discrete edits; no implicit crossfade or per-frame pixel admission occurs.
    pub fn interpolate(self, to: Self, alpha: f64) -> Result<Self, ImagePaintError> {
        if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
            return Err(ImagePaintError::InvalidProgress);
        }
        if self.fit != to.fit || self.address != to.address {
            return Err(ImagePaintError::DifferentMappingTopology);
        }
        if self
            .transform
            .scale
            .into_iter()
            .zip(to.transform.scale)
            .any(|(a, b)| a.is_sign_positive() != b.is_sign_positive())
        {
            return Err(ImagePaintError::SingularInterpolation);
        }
        if alpha == 0.0 {
            return Ok(self);
        }
        if alpha == 1.0 {
            return Ok(to);
        }
        let mix = |a: f64, b: f64| {
            if a.is_sign_positive() == b.is_sign_positive() {
                // Difference form preserves equal subnormal scales instead of
                // rounding both weighted endpoint contributions down to zero.
                alpha.mul_add(b - a, a)
            } else {
                // Opposite extremes can overflow b-a, but weighted terms cannot.
                a * (1.0 - alpha) + b * alpha
            }
        };
        let point = |a: [f64; 2], b: [f64; 2]| [mix(a[0], b[0]), mix(a[1], b[1])];
        Self::new(
            self.fit,
            point(self.alignment, to.alignment),
            self.address,
            ImageUvTransform::new(
                point(self.transform.translation, to.transform.translation),
                point(self.transform.scale, to.transform.scale),
                mix(self.transform.rotation, to.transform.rotation),
            )?,
        )
    }

    pub fn sample_premultiplied(
        self,
        image: &RasterImageResource,
        sampling: RasterImageSampling,
        local_size: [f64; 2],
        point: [f64; 2],
    ) -> Result<[f64; 4], ImagePaintError> {
        let Some(uv) = self.image_uv(point, local_size, [image.width(), image.height()])? else {
            return Ok([0.0; 4]);
        };
        Ok(sample(image, sampling, self.address, uv))
    }
}

/// Immutable paint parameters over one versioned resource capability. Resource
/// admission, retention, clone remapping and publication belong to the store.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImagePaint {
    resource: RasterImageResourceHandle,
    mapping: ImageMapping,
    sampling: RasterImageSampling,
}

impl ImagePaint {
    pub const fn new(
        resource: RasterImageResourceHandle,
        mapping: ImageMapping,
        sampling: RasterImageSampling,
    ) -> Self {
        Self {
            resource,
            mapping,
            sampling,
        }
    }
    pub const fn resource(self) -> RasterImageResourceHandle {
        self.resource
    }
    pub const fn mapping(self) -> ImageMapping {
        self.mapping
    }
    pub const fn sampling(self) -> RasterImageSampling {
        self.sampling
    }
    pub fn interpolate(self, to: Self, alpha: f64) -> Result<Self, ImagePaintError> {
        if self.resource != to.resource || self.sampling != to.sampling {
            return Err(ImagePaintError::DifferentResourceOrSampling);
        }
        Ok(Self::new(
            self.resource,
            self.mapping.interpolate(to.mapping, alpha)?,
            self.sampling,
        ))
    }
    pub fn sample_premultiplied(
        self,
        arena: &RasterImageResourceArena,
        local_size: [f64; 2],
        point: [f64; 2],
    ) -> Result<[f64; 4], ImagePaintError> {
        let image = arena
            .get(self.resource)
            .ok_or(ImagePaintError::MissingResource)?;
        self.mapping
            .sample_premultiplied(image, self.sampling, local_size, point)
    }
}

fn sample(
    image: &RasterImageResource,
    sampling: RasterImageSampling,
    address: ImageAddressMode,
    mut uv: [f64; 2],
) -> [f64; 4] {
    let dimensions = [image.width(), image.height()];
    for axis in 0..2 {
        uv[axis] = match address {
            ImageAddressMode::Clamp => uv[axis].clamp(0.0, 1.0),
            ImageAddressMode::Repeat => uv[axis].rem_euclid(1.0),
            ImageAddressMode::Transparent => {
                let support = 2.0 / f64::from(dimensions[axis]);
                if uv[axis] < -support || uv[axis] > 1.0 + support {
                    return [0.0; 4];
                }
                uv[axis]
            }
        };
    }
    let tap = |point: [i64; 2]| {
        let mut indices = [0; 2];
        for axis in 0..2 {
            let limit = i64::from(dimensions[axis]);
            indices[axis] = match address {
                ImageAddressMode::Clamp => point[axis].clamp(0, limit - 1),
                ImageAddressMode::Repeat => point[axis].rem_euclid(limit),
                ImageAddressMode::Transparent => {
                    if !(0..limit).contains(&point[axis]) {
                        return [0.0; 4];
                    }
                    point[axis]
                }
            };
        }
        let index = ((indices[1] as usize * image.width() as usize) + indices[0] as usize) * 4;
        let rgba = &image.rgba8()[index..index + 4];
        let alpha = f64::from(rgba[3]) / 255.0;
        [
            f64::from(rgba[0]) / 255.0 * alpha,
            f64::from(rgba[1]) / 255.0 * alpha,
            f64::from(rgba[2]) / 255.0 * alpha,
            alpha,
        ]
    };
    if sampling == RasterImageSampling::Nearest {
        return tap(std::array::from_fn(|axis| {
            (uv[axis] * f64::from(dimensions[axis])).floor() as i64
        }));
    }
    let position: [f64; 2] =
        std::array::from_fn(|axis| uv[axis] * f64::from(dimensions[axis]) - 0.5);
    let base = position.map(|v| v.floor() as i64);
    let fraction: [f64; 2] = std::array::from_fn(|axis| position[axis] - base[axis] as f64);
    let mut result = [0.0; 4];
    let range = if sampling == RasterImageSampling::Linear {
        0..=1
    } else {
        -1..=2
    };
    for y in range.clone() {
        for x in range.clone() {
            let weight = if sampling == RasterImageSampling::Linear {
                (if x == 0 {
                    1.0 - fraction[0]
                } else {
                    fraction[0]
                }) * (if y == 0 {
                    1.0 - fraction[1]
                } else {
                    fraction[1]
                })
            } else {
                cubic(f64::from(x) - fraction[0]) * cubic(f64::from(y) - fraction[1])
            };
            let pixel = tap([base[0] + i64::from(x), base[1] + i64::from(y)]);
            for channel in 0..4 {
                result[channel] += weight * pixel[channel];
            }
        }
    }
    result[3] = result[3].clamp(0.0, 1.0);
    for channel in 0..3 {
        result[channel] = result[channel].clamp(0.0, result[3]);
    }
    result
}

fn cubic(value: f64) -> f64 {
    let x = value.abs();
    if x <= 1.0 {
        (x - 2.0) * x * x + 1.0
    } else if x < 2.0 {
        ((-x + 5.0) * x - 8.0) * x + 4.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests;
