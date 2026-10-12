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
