//! Shared host value conversion; pixels and admission stay in the image API.
use super::AuthoringFailure;
use noon_core::RasterImageSampling;

pub fn image_sampling(value: &str) -> Result<RasterImageSampling, AuthoringFailure> {
    match value {
        "nearest" => Ok(RasterImageSampling::Nearest),
        "linear" | "bilinear" => Ok(RasterImageSampling::Linear),
        "bicubic" => Ok(RasterImageSampling::Bicubic),
        _ => Err(AuthoringFailure::new(
            "unsupported_operation",
            "image.sampling",
            "image sampling must be nearest, bilinear, or bicubic",
        )),
    }
}
