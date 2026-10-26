//! Optional language coercion into the shared immutable Rust paint value.
use crate::AuthoringFailure;
use noon::{
    Color, GradientStop, ImageAddressMode, ImageFit, ImageMapping, ImageUvTransform, LinearGradient,
};
use std::sync::Arc;

fn linear_gradient(
    start: &[f64],
    end: &[f64],
    stops: &[f64],
) -> Result<Arc<LinearGradient>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    let start: [f64; 2] = start
        .try_into()
        .map_err(|_| invalid("gradient start requires two coordinates"))?;
    let end: [f64; 2] = end
        .try_into()
        .map_err(|_| invalid("gradient end requires two coordinates"))?;
    if !stops.len().is_multiple_of(5) {
        return Err(invalid(
            "gradient stops require position and four color channels",
        ));
    }
    let stops = stops
        .as_chunks::<5>()
        .0
        .iter()
        .map(|stop| {
            if !stop[1..]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                return Err(invalid(
                    "gradient colors require finite RGBA channels in [0, 1]",
                ));
            }
            GradientStop::new(
                stop[0],
                Color::rgba(
                    stop[1] as f32,
                    stop[2] as f32,
                    stop[3] as f32,
                    stop[4] as f32,
                ),
            )
            .map_err(|error| invalid(&error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    LinearGradient::new(start, end, stops)
        .map(Arc::new)
        .map_err(|error| invalid(&error.to_string()))
}

fn image_mapping(
    fit: &str,
    alignment: &[f64],
    address: &str,
    translation: &[f64],
    scale: &[f64],
    rotation: f64,
) -> Result<ImageMapping, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_image", message);
    let fit = match fit {
        "cover" => ImageFit::Cover,
        "contain" => ImageFit::Contain,
        "stretch" => ImageFit::Stretch,
        _ => return Err(invalid("image fit must be cover, contain, or stretch")),
    };
    let address = match address {
        "clamp" => ImageAddressMode::Clamp,
        "repeat" => ImageAddressMode::Repeat,
        "transparent" => ImageAddressMode::Transparent,
        _ => {
            return Err(invalid(
                "image address must be clamp, repeat, or transparent",
            ))
        }
    };
    let coordinates = |value: &[f64], name: &str| {
        <[f64; 2]>::try_from(value)
            .map_err(|_| invalid(&format!("image {name} requires two coordinates")))
    };
    let transform = ImageUvTransform::new(
        coordinates(translation, "translation")?,
        coordinates(scale, "scale")?,
        rotation,
    )
    .map_err(|error| invalid(&error.to_string()))?;
    ImageMapping::new(
        fit,
        coordinates(alignment, "alignment")?,
        address,
        transform,
    )
    .map_err(|error| invalid(&error.to_string()))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle};
    use wasm_bindgen::prelude::*;

    /// Immutable paint value; construction allocates no scene or GPU identity.
    #[wasm_bindgen]
    pub struct WasmLinearGradient {
        pub(crate) value: Arc<LinearGradient>,
    }

    /// Immutable parameters retain the source's ordinary store capability.
    /// Pixels remain owned by the existing Rust resource arena.
    #[wasm_bindgen]
    pub struct WasmImagePaint {
        value: noon::ImagePaint,
        source: noon::Mobject,
    }

    impl WasmImagePaint {
        pub(crate) fn value_for(
            &self,
            target: &noon::Mobject,
        ) -> Result<noon::ImagePaint, AuthoringFailure> {
            if !std::rc::Rc::ptr_eq(self.source.integration_store(), target.integration_store()) {
                return Err(noon::AuthoringError::ForeignStore.into());
            }
            Ok(self.value)
        }
    }

    #[wasm_bindgen]
    impl WasmImagePaint {
        #[wasm_bindgen(constructor)]
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            image: &WasmAuthoringMobjectHandle,
            fit: &str,
            alignment: &[f64],
            address: &str,
            translation: &[f64],
            scale: &[f64],
            rotation: f64,
            sampling: Option<String>,
        ) -> Result<WasmImagePaint, JsValue> {
            let mapping = image_mapping(fit, alignment, address, translation, scale, rotation)
                .map_err(js_error)?;
            let sampling = sampling
                .as_deref()
                .map(crate::authoring_image::sampling)
                .transpose()?;
            let source = image.semantic_mobject().clone();
            let value = source.image_paint(mapping, sampling).map_err(js_error)?;
            Ok(Self { value, source })
        }
    }

    #[wasm_bindgen]
    impl WasmLinearGradient {
        #[wasm_bindgen(constructor)]
        pub fn new(
            start: &[f64],
            end: &[f64],
            stops: &[f64],
        ) -> Result<WasmLinearGradient, JsValue> {
            linear_gradient(start, end, stops)
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setFillImage)]
        pub fn set_fill_image(
            &self,
            paint: &WasmImagePaint,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let value = paint.value_for(self.semantic_mobject()).map_err(js_error)?;
            self.semantic_mobject()
                .clone()
                .set_fill_image(value, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillGradient)]
        pub fn set_fill_gradient(
            &self,
            gradient: &WasmLinearGradient,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_fill_gradient(gradient.value.clone(), opacity)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_checks_shape_and_f64_channels_before_narrowing() {
        let stops = [0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0];
        let paint = linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &stops).unwrap();
        assert_eq!(
            paint.sample_premultiplied([0.5, 0.5]).unwrap(),
            [0.5, 0.0, 0.5, 1.0]
        );
        assert!(linear_gradient(&[0.0], &[1.0, 0.5], &stops).is_err());
        assert!(linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &stops[..9]).is_err());
        let mut invalid = stops;
        invalid[1] = 1.0 + f64::EPSILON;
        assert!(linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &invalid).is_err());
    }

    #[test]
    fn image_boundary_preserves_checked_mapping_and_rejects_language_shape() {
        let mapping = image_mapping(
            "contain",
            &[0.25, 0.75],
            "repeat",
            &[-0.125, 0.5],
            &[-2.0, 3.0],
            0.25,
        )
        .unwrap();
        assert_eq!(mapping.fit(), ImageFit::Contain);
        assert_eq!(mapping.address(), ImageAddressMode::Repeat);
        assert_eq!(mapping.alignment(), [0.25, 0.75]);
        assert_eq!(mapping.transform().translation(), [-0.125, 0.5]);
        assert_eq!(mapping.transform().scale(), [-2.0, 3.0]);
        assert_eq!(mapping.transform().rotation(), 0.25);
        for (fit, address) in [("unknown", "clamp"), ("cover", "unknown")] {
            assert!(image_mapping(fit, &[0.5; 2], address, &[0.0; 2], &[1.0; 2], 0.0).is_err());
        }
        for (alignment, translation, scale) in [
            (&[0.5][..], &[0.0, 0.0][..], &[1.0, 1.0][..]),
            (&[0.5, 0.5], &[0.0], &[1.0, 1.0]),
            (&[0.5, 0.5], &[0.0, 0.0], &[1.0, 1.0, 1.0]),
        ] {
            assert!(image_mapping("cover", alignment, "clamp", translation, scale, 0.0).is_err());
        }
        assert!(image_mapping(
            "cover",
            &[1.0 + f64::EPSILON, 0.5],
            "clamp",
            &[0.0; 2],
            &[1.0; 2],
            0.0
        )
        .is_err());
        assert!(image_mapping("cover", &[0.5; 2], "clamp", &[0.0; 2], &[0.0, 1.0], 0.0).is_err());
        assert!(
            image_mapping("cover", &[0.5; 2], "clamp", &[0.0; 2], &[1.0; 2], f64::NAN).is_err()
        );
    }
}
