//! Bounded PNG/JPEG/WebP decode at resource preparation, never on a render-frame path.
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
    AnimatedImageUnsupported,
    InvalidEncoding(String),
    DecodedSize { width: u32, height: u32, limit: u64 },
}
impl std::fmt::Display for ImageDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidLimits => f.write_str("image decode limits must be positive"),
            Self::EncodedSize { actual, limit } => write!(f, "encoded image has {actual} bytes, exceeding the limit {limit}"),
            Self::UnsupportedFormat => f.write_str("only PNG, JPEG, and WebP raster files are supported; other formats require explicit qualification"),
            Self::AnimatedImageUnsupported => f.write_str("animated WebP images are not supported"),
            Self::InvalidEncoding(reason) => write!(f, "invalid PNG/JPEG/WebP image: {reason}"),
            Self::DecodedSize { width, height, limit } => write!(f, "decoded image {width}x{height} exceeds the RGBA8 byte limit {limit}"),
        }
    }
}
impl std::error::Error for ImageDecodeError {}

impl ImageMobjectOptions {
    /// Decode a PNG/JPEG/WebP resource with default bounded allocation. File/URL/blob
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
            Ok(format @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) => format,
            _ => return Err(ImageDecodeError::UnsupportedFormat.into()),
        };
        let (width, height, rgba) = if format == ImageFormat::WebP {
            let decoder =
                image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(decode_error)?;
            if decoder.has_animation() {
                return Err(ImageDecodeError::AnimatedImageUnsupported.into());
            }
            decode_rgba8(decoder, limits)?
        } else {
            let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
            reader.limits(allocation_limits(limits));
            let decoder = reader.into_decoder().map_err(decode_error)?;
            decode_rgba8(decoder, limits)?
        };
        Self::rgba8(width, height, rgba)
    }
}

fn allocation_limits(limits: ImageDecodeLimits) -> image::Limits {
    let mut allocation_limits = image::Limits::default();
    allocation_limits.max_image_width = Some(limits.max_dimension);
    allocation_limits.max_image_height = Some(limits.max_dimension);
    allocation_limits.max_alloc = Some(limits.max_decoded_bytes);
    allocation_limits
}

fn decode_rgba8(
    mut decoder: impl ImageDecoder,
    limits: ImageDecodeLimits,
) -> Result<(u32, u32, Vec<u8>), AuthoringError> {
    decoder
        .set_limits(allocation_limits(limits))
        .map_err(decode_error)?;
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
        .into_rgba8()
        .into_raw();
    Ok((width, height, rgba))
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
    fn lossless_webp_decodes_to_canonical_rgba8() {
        let expected = [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255,
        ];
        let mut webp = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut webp)
            .write_image(&expected, 2, 2, ExtendedColorType::Rgba8)
            .unwrap();

        let options = ImageMobjectOptions::encoded(&webp).unwrap();
        assert_eq!((options.pixel_width(), options.pixel_height()), (2, 2));
        let mut scene = crate::Scene::new();
        let image = scene.image(options).unwrap();
        let handle = image.state().unwrap().content.image().unwrap().resource();
        let store = scene.integration_store().borrow();
        assert_eq!(
            store.raster_image_resources().get(handle).unwrap().rgba8(),
            &expected,
        );
    }

    #[test]
    fn animated_webp_is_rejected_instead_of_silently_using_its_first_frame() {
        // Two-frame 2x2 lossless WebP generated by Pillow 11.3.0.
        let animated_webp = [
            0x52, 0x49, 0x46, 0x46, 0x84, 0x00, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50,
            0x38, 0x58, 0x0a, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01,
            0x00, 0x00, 0x41, 0x4e, 0x49, 0x4d, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x41, 0x4e, 0x4d, 0x46, 0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x64, 0x00, 0x00, 0x02, 0x56, 0x50,
            0x38, 0x4c, 0x0f, 0x00, 0x00, 0x00, 0x2f, 0x01, 0x40, 0x00, 0x00, 0x07, 0x10, 0xfd,
            0x8f, 0xfe, 0x07, 0x22, 0xa2, 0xff, 0x01, 0x00, 0x41, 0x4e, 0x4d, 0x46, 0x28, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00,
            0x64, 0x00, 0x00, 0x00, 0x56, 0x50, 0x38, 0x4c, 0x0f, 0x00, 0x00, 0x00, 0x2f, 0x01,
            0x40, 0x00, 0x00, 0x07, 0x10, 0xd1, 0xff, 0xfe, 0x07, 0x22, 0xa2, 0xff, 0x01, 0x00,
        ];
        let error = ImageMobjectOptions::encoded(&animated_webp).unwrap_err();
        assert!(error
            .to_string()
            .contains("animated WebP images are not supported"));
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
