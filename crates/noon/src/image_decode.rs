//! Bounded PNG/JPEG decode at resource preparation, never on a render-frame path.
use crate::{AuthoringError, ImageMobjectOptions};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use std::io::Cursor;

/// Host-side decode limits, independent of a renderer's potentially smaller limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDecodeLimits {
    pub max_dimension: u32,
    pub max_encoded_bytes: usize,
    pub max_decoded_bytes: u64,
}
impl Default for ImageDecodeLimits {
    fn default() -> Self {
        Self {
            max_dimension: 16_384,
            max_encoded_bytes: 32 * 1024 * 1024,
            max_decoded_bytes: 64 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageDecodeError {
    InvalidLimits,
    EncodedSize { actual: usize, limit: usize },
    UnsupportedFormat,
    InvalidEncoding(String),
    DecodedSize { width: u32, height: u32, limit: u64 },
}
impl std::fmt::Display for ImageDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("image decode limits must be positive"),
            Self::EncodedSize { actual, limit } => write!(f, "encoded image has {actual} bytes, exceeding the limit {limit}"),
            Self::UnsupportedFormat => f.write_str("only PNG and JPEG raster files are supported; other formats require explicit qualification"),
            Self::InvalidEncoding(reason) => write!(f, "invalid PNG/JPEG image: {reason}"),
            Self::DecodedSize { width, height, limit } => write!(f, "decoded image {width}x{height} exceeds the RGBA8 byte limit {limit}"),
        }
    }
}
impl std::error::Error for ImageDecodeError {}

impl ImageMobjectOptions {
    /// Decode a PNG/JPEG resource with default bounded allocation. File/URL/blob
    /// access stays with the host; identical decoded pixels deduplicate on admission.
    pub fn encoded(bytes: &[u8]) -> Result<Self, AuthoringError> {
        Self::encoded_with_limits(bytes, ImageDecodeLimits::default())
    }

    pub fn encoded_with_limits(
        bytes: &[u8],
        limits: ImageDecodeLimits,
    ) -> Result<Self, AuthoringError> {
        if limits.max_dimension == 0
            || limits.max_encoded_bytes == 0
            || limits.max_decoded_bytes == 0
        {
            return Err(ImageDecodeError::InvalidLimits.into());
        }
        if bytes.len() > limits.max_encoded_bytes {
            return Err(ImageDecodeError::EncodedSize {
                actual: bytes.len(),
                limit: limits.max_encoded_bytes,
            }
            .into());
        }
        let format = match image::guess_format(bytes) {
            Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg)) => format,
            _ => return Err(ImageDecodeError::UnsupportedFormat.into()),
        };
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut allocation_limits = image::Limits::default();
        allocation_limits.max_image_width = Some(limits.max_dimension);
        allocation_limits.max_image_height = Some(limits.max_dimension);
        allocation_limits.max_alloc = Some(limits.max_decoded_bytes);
        reader.limits(allocation_limits);
        let decoder = reader.into_decoder().map_err(decode_error)?;
        let (width, height) = decoder.dimensions();
        let rgba_bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|count| count.checked_mul(4));
        if !rgba_bytes.is_some_and(|count| count <= limits.max_decoded_bytes) {
            return Err(ImageDecodeError::DecodedSize {
                width,
                height,
                limit: limits.max_decoded_bytes,
            }
            .into());
        }
        let rgba = DynamicImage::from_decoder(decoder)
            .map_err(decode_error)?
            .into_rgba8();
        Self::rgba8(width, height, rgba.into_raw())
    }
}
fn decode_error(error: image::ImageError) -> AuthoringError {
    ImageDecodeError::InvalidEncoding(error.to_string()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ExtendedColorType, ImageEncoder};

    #[test]
    fn png_and_jpeg_decode_to_canonical_rgba8() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(
                &[255, 0, 0, 255, 0, 255, 0, 128],
                2,
                1,
                ExtendedColorType::Rgba8,
            )
            .unwrap();
        let options = ImageMobjectOptions::encoded(&png).unwrap();
        let mut scene = crate::Scene::new();
        let image = scene.image(options).unwrap();
        let handle = image.state().unwrap().content.image().unwrap().resource();
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .raster_image_resources()
                .get(handle)
                .unwrap()
                .rgba8(),
            &[255, 0, 0, 255, 0, 255, 0, 128]
        );
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode(&[90, 90, 90, 90, 90, 90], 2, 1, ExtendedColorType::Rgb8)
            .unwrap();
        let options = ImageMobjectOptions::encoded(&jpeg).unwrap();
        assert_eq!((options.pixel_width(), options.pixel_height()), (2, 1));
        let image = scene.image(options).unwrap();
        let handle = image.state().unwrap().content.image().unwrap().resource();
        let store = scene.integration_store().borrow();
        let pixels = store.raster_image_resources().get(handle).unwrap().rgba8();
        assert_eq!(pixels[3], 255);
        assert_eq!(pixels[7], 255);
        assert!(pixels[0].abs_diff(90) <= 2);
    }

    #[test]
    fn decode_rejects_limits_unsupported_and_truncated_inputs_before_admission() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0; 64], 4, 4, ExtendedColorType::Rgba8)
            .unwrap();
        for limits in [
            ImageDecodeLimits {
                max_dimension: 0,
                ..Default::default()
            },
            ImageDecodeLimits {
                max_dimension: 2,
                ..Default::default()
            },
            ImageDecodeLimits {
                max_encoded_bytes: 8,
                ..Default::default()
            },
            ImageDecodeLimits {
                max_decoded_bytes: 32,
                ..Default::default()
            },
        ] {
            assert!(ImageMobjectOptions::encoded_with_limits(&png, limits).is_err());
        }
        for bytes in [
            &b"GIF89a"[..],
            &b"RIFFabcdWEBP"[..],
            &b"not an image"[..],
            &png[..20],
        ] {
            assert!(ImageMobjectOptions::encoded(bytes).is_err());
        }
    }
}
