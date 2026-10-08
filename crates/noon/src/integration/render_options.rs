//! Shared output-option meanings for compiled Rust, CPython and Rust/WASM.
//!
//! This module resolves values, not source execution, frame scheduling, GPU or
//! filesystem policy. Frontends parse flag syntax and pass optional overrides.
//! The supported preset values follow the pinned ManimCE v0.21.0 constants.

use std::{error::Error, fmt};

use super::FrameRate;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderFormat {
    Mp4,
    PngSequence,
}

impl RenderFormat {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::PngSequence => "png",
        }
    }
}

/// Unresolved frontend values. Absence is different from an invalid empty value.
/// Explicit dimensions and frame rate independently override a quality preset.
/// `resolution` and individual pixel dimensions cannot be supplied together.
#[derive(Clone, Copy, Debug, Default)]
pub struct RenderOptionInputs<'a> {
    pub quality: Option<&'a str>,
    pub resolution: Option<&'a str>,
    pub frame_rate: Option<&'a str>,
    pub format: Option<&'a str>,
    pub pixel_width: Option<u32>,
    pub pixel_height: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedRenderOptions {
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub frame_rate: FrameRate,
    pub format: RenderFormat,
}

impl RenderOptionInputs<'_> {
    pub fn resolve(self) -> Result<ResolvedRenderOptions, RenderOptionsError> {
        let (width, height, fps) = match self.quality.unwrap_or("h") {
            "l" | "low_quality" => (854, 480, 15),
            "m" | "medium_quality" => (1280, 720, 30),
            "h" | "high_quality" => (1920, 1080, 60),
            "p" | "production_quality" => (2560, 1440, 60),
            "k" | "fourk_quality" => (3840, 2160, 60),
            _ => return Err(RenderOptionsError::Quality),
        };
        let (pixel_width, pixel_height) = if let Some(resolution) = self.resolution {
            if self.pixel_width.is_some() || self.pixel_height.is_some() {
                return Err(RenderOptionsError::ConflictingResolution);
            }
            let (width, height) = resolution
                .split_once(',')
                .ok_or(RenderOptionsError::Resolution)?;
            (dimension(width)?, dimension(height)?)
        } else {
            let size = (
                self.pixel_width.unwrap_or(width),
                self.pixel_height.unwrap_or(height),
            );
            if size.0 == 0 || size.1 == 0 {
                return Err(RenderOptionsError::Resolution);
            }
            size
        };
        let frame_rate = match self.frame_rate {
            Some(value) => parse_render_frame_rate(value)?,
            None => FrameRate::new(fps, 1).map_err(|_| RenderOptionsError::FrameRate)?,
        };
        let format = match self.format.unwrap_or("mp4") {
            "mp4" => RenderFormat::Mp4,
            "png" => RenderFormat::PngSequence,
            _ => return Err(RenderOptionsError::UnsupportedFormat),
        };
        Ok(ResolvedRenderOptions {
            pixel_width,
            pixel_height,
            frame_rate,
            format,
        })
    }
}

fn dimension(text: &str) -> Result<u32, RenderOptionsError> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(RenderOptionsError::Resolution);
    }
    text.parse::<u32>()
        .ok()
        .filter(|&value| value > 0)
        .ok_or(RenderOptionsError::Resolution)
}

/// Parse numeric FPS exactly, without accumulating floating-point error.
///
/// Decimal `29.97` is exactly 2997/100, NOT an implicit 30000/1001 conversion.
/// The explicit rational syntax is a Noon extension. Scientific notation is
/// accepted for numeric frontends; malformed/zero/negative/nonfinite/overflowing
/// rates fail, never clamp. The reduced ratio must fit the existing FrameRate.
pub fn parse_render_frame_rate(text: &str) -> Result<FrameRate, RenderOptionsError> {
    let text = text.trim();
    if text.is_empty() || text.len() > 128 {
        return Err(RenderOptionsError::FrameRate);
    }
    let (numerator, denominator) = if let Some((p, q)) = text.split_once('/') {
        (unsigned(p.trim())?, unsigned(q.trim())?)
    } else {
        let text = text.strip_prefix('+').unwrap_or(text);
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(index) => {
                let exponent = text[index + 1..]
                    .parse::<i32>()
                    .ok()
                    .filter(|value| (-38..=38).contains(value))
                    .ok_or(RenderOptionsError::FrameRate)?;
                (&text[..index], exponent)
            }
            None => (text, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if whole.is_empty() && fraction.is_empty() {
            return Err(RenderOptionsError::FrameRate);
        }
        if !whole.bytes().chain(fraction.bytes()).all(|c| c.is_ascii_digit()) {
            return Err(RenderOptionsError::FrameRate);
        }
        // Trimming insignificant zeros avoids rejecting e.g. 60.000... solely
        // because an otherwise reducible denominator would overflow u128.
        let fraction = fraction.trim_end_matches('0');
        let mut digits = whole.bytes().chain(fraction.bytes());
        let mut numerator = digits.try_fold(0_u128, |value, c| {
            value.checked_mul(10)?.checked_add(u128::from(c - b'0'))
        }).ok_or(RenderOptionsError::FrameRate)?;
        let scale = i32::try_from(fraction.len())
            .map_err(|_| RenderOptionsError::FrameRate)? - exponent;
        let denominator = if scale >= 0 {
            10_u128.checked_pow(scale as u32).ok_or(RenderOptionsError::FrameRate)?
        } else {
            numerator = numerator.checked_mul(
                10_u128.checked_pow((-scale) as u32).ok_or(RenderOptionsError::FrameRate)?
            ).ok_or(RenderOptionsError::FrameRate)?;
            1
        };
        (numerator, denominator)
    };
    if numerator == 0 || denominator == 0 {
        return Err(RenderOptionsError::FrameRate);
    }
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let p = u32::try_from(numerator / a).map_err(|_| RenderOptionsError::FrameRate)?;
    let q = u32::try_from(denominator / a).map_err(|_| RenderOptionsError::FrameRate)?;
    FrameRate::new(p, q).map_err(|_| RenderOptionsError::FrameRate)
}

fn unsigned(text: &str) -> Result<u128, RenderOptionsError> {
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(RenderOptionsError::FrameRate);
    }
    text.parse().map_err(|_| RenderOptionsError::FrameRate)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderOptionsError {
    Quality,
    Resolution,
    ConflictingResolution,
    FrameRate,
    UnsupportedFormat,
}

impl fmt::Display for RenderOptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Quality => "quality must be l, m, h, p or k (or the corresponding Manim quality name)",
            Self::Resolution => "resolution must contain positive integer width,height within u32",
            Self::ConflictingResolution => "resolution cannot be combined with individual pixel dimensions",
            Self::FrameRate => "frame rate must be a positive representable number or integer numerator/denominator",
            Self::UnsupportedFormat => "unsupported render format: this output profile supports mp4 and png only",
        })
    }
}

impl Error for RenderOptionsError {}

// Project only failures at optional language boundaries through the existing
// shared diagnostic representation. Direct Rust callers keep the typed error.
impl From<RenderOptionsError> for super::AuthoringFailure {
    fn from(error: RenderOptionsError) -> Self {
        use RenderOptionsError as E;
        let (category, code) = match error {
            E::Quality => ("invalid_input", "render.quality"),
            E::Resolution => ("invalid_input", "render.resolution"),
            E::ConflictingResolution => ("invalid_input", "render.conflicting_resolution"),
            E::FrameRate => ("invalid_input", "render.frame_rate"),
            E::UnsupportedFormat => ("unsupported_operation", "render.unsupported_format"),
        };
        Self::new(category, code, error)
    }
}

#[cfg(test)]
mod tests;
