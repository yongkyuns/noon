//! Consumable inert image input; shared Rust owns decoding and live admission.
use crate::{engine_error, mobject::MobjectHandle};
use pyo3::prelude::*;

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct ImageOptions {
    options: Option<noon::ImageMobjectOptions>,
}
impl ImageOptions {
    fn value(&mut self) -> PyResult<&mut noon::ImageMobjectOptions> {
        self.options
            .as_mut()
            .ok_or_else(|| engine_error("image input has been consumed"))
    }
    pub(crate) fn take(&mut self) -> PyResult<noon::ImageMobjectOptions> {
        self.options
            .take()
            .ok_or_else(|| engine_error("image input has been consumed"))
    }
}
#[pymethods]
impl ImageOptions {
    #[staticmethod]
    fn rgba8(width: u32, height: u32, pixels: &[u8]) -> PyResult<Self> {
        noon::ImageMobjectOptions::rgba8(width, height, pixels)
            .map(|options| Self {
                options: Some(options),
            })
            .map_err(engine_error)
    }
    #[staticmethod]
    fn encoded(bytes: &[u8]) -> PyResult<Self> {
        #[cfg(feature = "image-decode")]
        {
            noon::ImageMobjectOptions::encoded(bytes)
                .map(|options| Self {
                    options: Some(options),
                })
                .map_err(engine_error)
        }
        #[cfg(not(feature = "image-decode"))]
        {
            let _ = bytes;
            Err(engine_error(noon::integration::AuthoringFailure::new(
                "unsupported_operation",
                "image.decoder_unavailable",
                "encoded image input requires the optional image-decode feature",
            )))
        }
    }
    #[pyo3(name = "setScaleToResolution")]
    fn set_scale_to_resolution(&mut self, value: f64) -> PyResult<()> {
        self.value()?
            .set_scale_to_resolution(value)
            .map_err(engine_error)
    }
    #[pyo3(name = "setFrameHeight")]
    fn set_frame_height(&mut self, value: f64) -> PyResult<()> {
        self.value()?.set_frame_height(value).map_err(engine_error)
    }
    #[pyo3(name = "setHeight")]
    fn set_height(&mut self, value: f64) -> PyResult<()> {
        self.value()?.set_height(value).map_err(engine_error)
    }
    #[pyo3(name = "setOpacity")]
    fn set_opacity(&mut self, value: f64) -> PyResult<()> {
        self.value()?.set_opacity(value).map_err(engine_error)
    }
    #[pyo3(name = "setZIndex")]
    fn set_z_index(&mut self, value: f64) -> PyResult<()> {
        self.value()?.set_z_index(value).map_err(engine_error)
    }
    #[pyo3(name = "setSampling")]
    fn set_sampling(&mut self, value: &str) -> PyResult<()> {
        let sampling = noon::integration::image_sampling(value).map_err(engine_error)?;
        self.value()?.set_sampling(sampling);
        Ok(())
    }
    fn free(&mut self) {
        self.options.take();
    }
}

pub(crate) fn live(
    context: &mut crate::context::Context,
    input: &mut ImageOptions,
) -> PyResult<MobjectHandle> {
    // Match consumed WASM arguments on success and rejection.
    let options = input.take()?;
    context.available()?;
    if context.execution.is_some() {
        context.with_live(|live| live.create_image(options))
    } else {
        noon::Mobject::from_image(
            std::rc::Rc::clone(context.scene.integration_store()),
            options,
        )
        .map_err(engine_error)
    }
    .map(MobjectHandle::from_semantic_mobject)
}
