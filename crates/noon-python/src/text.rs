//! Native resource inputs and thin constructor projections. Rust owns shaping,
//! font/resource identity, layout, and atomic authored/live admission.
use crate::{
    context::{Context, Store},
    engine_error,
    mobject::MobjectHandle,
};
use pyo3::prelude::*;
use std::{rc::Rc, sync::Arc};

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct FontFace {
    face: noon::NativeFontFace,
}
#[pymethods]
impl FontFace {
    #[new]
    fn new(family: &str, data: &[u8], face_index: u32) -> PyResult<Self> {
        noon::NativeFontFace::new(family, Arc::<[u8]>::from(data), face_index)
            .map(|face| Self { face })
            .map_err(|error| {
                engine_error(noon::integration::AuthoringFailure::new(
                    "invalid_input",
                    "text.invalid_font_face",
                    error,
                ))
            })
    }
    #[getter]
    fn family(&self) -> &str {
        &self.face.family
    }
    #[getter(faceIndex)]
    fn face_index(&self) -> u32 {
        self.face.face_index
    }
    #[pyo3(name = "cloneFace")]
    fn clone_face(&self) -> Self {
        Self {
            face: self.face.clone(),
        }
    }
    #[pyo3(name = "withMetricsProfile")]
    fn with_metrics_profile(&self, profile: &str) -> PyResult<Self> {
        let profile = match profile {
            "cairo-freetype" => noon::NativeFontMetricsProfile::CairoFreeType,
            "cairo-coretext" => noon::NativeFontMetricsProfile::CairoCoreText,
            _ => {
                return Err(engine_error(noon::integration::AuthoringFailure::new(
                    "invalid_input",
                    "text.invalid_metrics_profile",
                    "unknown native font metrics profile",
                )))
            }
        };
        self.face
            .clone()
            .with_metrics_profile(profile)
            .map(|face| Self { face })
            .map_err(|error| {
                engine_error(noon::integration::AuthoringFailure::new(
                    "invalid_input",
                    "text.unsupported_metrics_profile",
                    error,
                ))
            })
    }
}
#[pyfunction]
fn bundled_native_font_face(family: &str) -> PyResult<FontFace> {
    noon::bundled_native_font_face(family)
        .map(|face| FontFace { face })
        .map_err(engine_error)
}

type Colors = (noon::Color, Vec<(Arc<str>, noon::Color)>);
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub struct TextColorBatch {
    value: Option<Colors>,
}
impl TextColorBatch {
    fn value(&mut self) -> PyResult<&mut Colors> {
        self.value
            .as_mut()
            .ok_or_else(|| engine_error("text color input has been consumed"))
    }
}
fn color(r: f64, g: f64, b: f64, a: f64) -> PyResult<noon::Color> {
    use noon::integration::text_color_channel as c;
    Ok(noon::Color::rgba(
        c(r).map_err(engine_error)?,
        c(g).map_err(engine_error)?,
        c(b).map_err(engine_error)?,
        c(a).map_err(engine_error)?,
    ))
}
#[pymethods]
impl TextColorBatch {
    #[new]
    fn new() -> Self {
        Self {
            value: Some((noon::WHITE, Vec::new())),
        }
    }
    #[pyo3(name = "setBaseColor")]
    fn set_base_color(&mut self, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        let next = color(r, g, b, a)?;
        self.value()?.0 = next;
        Ok(())
    }
    fn push(&mut self, selector: &str, r: f64, g: f64, b: f64, a: f64) -> PyResult<()> {
        let next = color(r, g, b, a)?;
        self.value()?.1.push((Arc::from(selector), next));
        Ok(())
    }
    fn free(&mut self) {
        self.value.take();
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    source: &str,
    family: &str,
    size: f64,
    spacing: f64,
    mut colors: Option<PyRefMut<'_, TextColorBatch>>,
    face: Option<PyRef<'_, FontFace>>,
    markup: bool,
) -> PyResult<noon::Text> {
    let text = noon::integration::native_text_options(
        source,
        family,
        size,
        spacing,
        face.map(|f| f.face.clone()),
        markup,
    )
    .map_err(engine_error)?;
    if let Some(batch) = colors.as_mut() {
        let (base, ranges) = batch
            .value
            .take()
            .ok_or_else(|| engine_error("text color input has been consumed"))?;
        Ok(text.color(base).with_text2color(ranges))
    } else {
        Ok(text)
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn authored(
    store: &Store,
    source: &str,
    family: &str,
    size: f64,
    spacing: f64,
    colors: Option<PyRefMut<'_, TextColorBatch>>,
    face: Option<PyRef<'_, FontFace>>,
    markup: bool,
) -> PyResult<MobjectHandle> {
    let text = prepare(source, family, size, spacing, colors, face, markup)?;
    noon::Mobject::from_text(Rc::clone(&store.semantics), text)
        .map(MobjectHandle::from_semantic_mobject)
        .map_err(engine_error)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn live(
    context: &mut Context,
    source: &str,
    family: &str,
    size: f64,
    spacing: f64,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    opacity: f64,
    colors: Option<PyRefMut<'_, TextColorBatch>>,
    face: Option<PyRef<'_, FontFace>>,
    markup: bool,
) -> PyResult<MobjectHandle> {
    context.available()?;
    let text = prepare(source, family, size, spacing, colors, face, markup)?;
    let opacity = noon::integration::text_color_channel(opacity).map_err(engine_error)?;
    let text = text.color(color(r, g, b, a)?).set_opacity(opacity);
    if context.execution.is_some() {
        context.with_live(|live| live.create_text(text))
    } else {
        noon::Mobject::from_text(Rc::clone(context.scene.integration_store()), text)
            .map_err(engine_error)
    }
    .map(MobjectHandle::from_semantic_mobject)
}
pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<FontFace>()?;
    module.add_class::<TextColorBatch>()?;
    module.add_function(wrap_pyfunction!(bundled_native_font_face, module)?)?;
    Ok(())
}
