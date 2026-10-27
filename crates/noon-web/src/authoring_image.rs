//! Thin typed image preparation/admission boundary. Pixels are never JSON frame state.
use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmImageMobjectOptions {
    pub(crate) options: noon::ImageMobjectOptions,
}

pub(crate) fn sampling(value: &str) -> Result<noon::RasterImageSampling, JsValue> {
    match value {
        "nearest" => Ok(noon::RasterImageSampling::Nearest),
        "linear" | "bilinear" => Ok(noon::RasterImageSampling::Linear),
        "bicubic" => Ok(noon::RasterImageSampling::Bicubic),
        _ => Err(js_error(crate::AuthoringFailure::new(
            "unsupported_operation",
            "image.sampling",
            "image sampling must be nearest, bilinear, or bicubic",
        ))),
    }
}

#[wasm_bindgen]
impl WasmImageMobjectOptions {
    #[wasm_bindgen(js_name = rgba8)]
    pub fn rgba8(width: u32, height: u32, pixels: &[u8]) -> Result<Self, JsValue> {
        noon::ImageMobjectOptions::rgba8(width, height, pixels)
            .map(|options| Self { options })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = encoded)]
    pub fn encoded(bytes: &[u8]) -> Result<Self, JsValue> {
        noon::ImageMobjectOptions::encoded(bytes)
            .map(|options| Self { options })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setScaleToResolution)]
    pub fn set_scale_to_resolution(&mut self, value: f64) -> Result<(), JsValue> {
        self.options
            .set_scale_to_resolution(value)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setFrameHeight)]
    pub fn set_frame_height(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_frame_height(value).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setHeight)]
    pub fn set_height(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_height(value).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_opacity(value).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setZIndex)]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), JsValue> {
        self.options.set_z_index(value).map_err(js_error)
    }
    #[wasm_bindgen(js_name = setSampling)]
    pub fn set_sampling(&mut self, value: &str) -> Result<(), JsValue> {
        self.options.set_sampling(sampling(value)?);
        Ok(())
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createImage)]
    pub fn create_image(
        &self,
        candidate: WasmImageMobjectOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        noon::Mobject::from_image(Rc::clone(&self.semantics), candidate.options)
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = imagePixelWidth)]
    pub fn image_pixel_width(&self) -> Result<u32, JsValue> {
        self.semantic_mobject()
            .image_dimensions()
            .map(|(width, _)| width)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = imagePixelHeight)]
    pub fn image_pixel_height(&self) -> Result<u32, JsValue> {
        self.semantic_mobject()
            .image_dimensions()
            .map(|(_, height)| height)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setImageSampling)]
    pub fn set_image_sampling(&self, value: &str) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .set_image_sampling(sampling(value)?)
            .map_err(js_error)
    }
}

/// Checked immutable mapping syntax; all fitting/interpolation lives in noon-core.
#[wasm_bindgen]
pub struct WasmImageMapping {
    pub(crate) value: noon::ImageMapping,
}

#[wasm_bindgen]
impl WasmImageMapping {
    #[wasm_bindgen(constructor)]
    pub fn new(
        fit: &str,
        alignment: &[f64],
        address: &str,
        translation: &[f64],
        scale: &[f64],
        rotation: f64,
    ) -> Result<Self, JsValue> {
        let invalid = |message: &str| {
            js_error(crate::AuthoringFailure::new(
                "invalid_input",
                "paint.image_mapping",
                message,
            ))
        };
        let fit = match fit {
            "cover" => noon::ImageFit::Cover,
            "contain" => noon::ImageFit::Contain,
            "stretch" => noon::ImageFit::Stretch,
            _ => return Err(invalid("image fit must be cover, contain, or stretch")),
        };
        let address = match address {
            "transparent" => noon::ImageAddressMode::Transparent,
            "clamp" => noon::ImageAddressMode::Clamp,
            "repeat" => noon::ImageAddressMode::Repeat,
            _ => {
                return Err(invalid(
                    "image address must be transparent, clamp, or repeat",
                ))
            }
        };
        let pair = |value: &[f64]| -> Result<[f64; 2], JsValue> {
            value
                .try_into()
                .map_err(|_| invalid("image mapping requires two coordinates per vector"))
        };
        let transform = noon::ImageUvTransform::new(pair(translation)?, pair(scale)?, rotation)
            .map_err(|error| invalid(&error.to_string()))?;
        let value = noon::ImageMapping::new(fit, pair(alignment)?, address, transform)
            .map_err(|error| invalid(&error.to_string()))?;
        Ok(Self { value })
    }
}

/// Inert strong pixel value, sharing the ordinary bitmap loader and arena.
#[wasm_bindgen]
pub struct WasmImagePaintOptions {
    pub(crate) options: noon::ImagePaintOptions,
}

#[wasm_bindgen]
impl WasmImagePaintOptions {
    #[wasm_bindgen(js_name = rgba8)]
    pub fn rgba8(width: u32, height: u32, pixels: &[u8]) -> Result<Self, JsValue> {
        noon::ImagePaintOptions::rgba8(width, height, pixels)
            .map(|options| Self { options })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = encoded)]
    pub fn encoded(bytes: &[u8]) -> Result<Self, JsValue> {
        noon::ImagePaintOptions::encoded(bytes)
            .map(|options| Self { options })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = fromImage)]
    pub fn from_image(image: &WasmAuthoringMobjectHandle) -> Result<Self, JsValue> {
        noon::ImagePaintOptions::from_image(image.semantic_mobject())
            .map(|options| Self { options })
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = copy)]
    pub fn copy(&self) -> Self {
        Self {
            options: self.options.clone(),
        }
    }
    #[wasm_bindgen(js_name = setMapping)]
    pub fn set_mapping(&mut self, mapping: &WasmImageMapping) {
        self.options.set_mapping(mapping.value);
    }
    #[wasm_bindgen(js_name = setSampling)]
    pub fn set_sampling(&mut self, value: &str) -> Result<(), JsValue> {
        self.options.set_sampling(sampling(value)?);
        Ok(())
    }
}

#[wasm_bindgen]
impl WasmAuthoringMobjectHandle {
    #[wasm_bindgen(js_name = setFillImage)]
    pub fn set_fill_image(
        &self,
        paint: &WasmImagePaintOptions,
        opacity: Option<f64>,
    ) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .set_fill_image(paint.options.clone(), opacity)
            .map_err(js_error)
    }
    #[wasm_bindgen(js_name = setFillImageMapping)]
    pub fn set_fill_image_mapping(
        &self,
        mapping: &WasmImageMapping,
        opacity: Option<f64>,
    ) -> Result<(), JsValue> {
        self.semantic_mobject()
            .clone()
            .set_fill_image_mapping(mapping.value, opacity)
            .map_err(js_error)
    }
}
