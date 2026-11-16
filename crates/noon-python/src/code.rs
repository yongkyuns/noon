//! CPython handles for the shared Rust Code constructor and retained listing.
use crate::{
    context::{Context, Store},
    engine_error,
    family::FamilyHandle,
    text::FontFace,
};
use noon::integration::code_input;
use pyo3::prelude::*;
use std::rc::Rc;

#[pyclass(skip_from_py_object, module = "_noon_native")]
struct CodeOptions(noon::CodeOptions);

#[pymethods]
impl CodeOptions {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (language, source_path, formatter_style, tab_width, add_line_numbers, line_numbers_from, background))]
    fn new(
        language: Option<String>,
        source_path: Option<String>,
        formatter_style: String,
        tab_width: f64,
        add_line_numbers: bool,
        line_numbers_from: f64,
        background: &str,
    ) -> PyResult<Self> {
        code_input::options(
            language,
            source_path,
            formatter_style,
            tab_width,
            add_line_numbers,
            line_numbers_from,
            background,
        )
        .map(Self)
        .map_err(engine_error)
    }

    #[pyo3(name = "setParagraph")]
    fn set_paragraph(
        &mut self,
        font: String,
        font_size: f64,
        line_spacing: f64,
        disable_ligatures: bool,
        font_face: Option<&FontFace>,
    ) -> PyResult<()> {
        self.0.paragraph_config = code_input::paragraph(
            font,
            font_size,
            line_spacing,
            disable_ligatures,
            font_face.map(|input| input.face.clone()),
        )
        .map_err(engine_error)?;
        Ok(())
    }

    #[pyo3(name = "setBackgroundGeometry")]
    fn set_background_geometry(&mut self, buff: f64, corner_radius: f64) {
        self.0.background_config.buff = buff;
        self.0.background_config.corner_radius = corner_radius;
    }

    #[pyo3(name = "setBackgroundOpacity")]
    fn set_background_opacity(&mut self, opacity: f64) {
        self.0.background_config.fill_opacity = opacity;
    }

    #[pyo3(name = "setBackgroundFill")]
    fn set_background_fill(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> PyResult<()> {
        self.0.background_config.fill_color =
            Some(code_input::color(red, green, blue, alpha).map_err(engine_error)?);
        Ok(())
    }

    #[pyo3(name = "setBackgroundStroke")]
    fn set_background_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        width: f64,
        opacity: f64,
    ) -> PyResult<()> {
        self.0.background_config.stroke_color =
            code_input::color(red, green, blue, alpha).map_err(engine_error)?;
        self.0.background_config.stroke_width = width;
        self.0.background_config.stroke_opacity = opacity;
        Ok(())
    }

    fn free(&self) {}
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
struct CodeHandle(noon::Code);

#[pymethods]
impl CodeHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    fn background(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.background().clone())
    }

    #[pyo3(name = "codeLines")]
    fn code_lines(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.code_lines().clone())
    }

    #[pyo3(name = "lineNumbers")]
    fn line_numbers(&self) -> Option<FamilyHandle> {
        self.0
            .line_numbers()
            .cloned()
            .map(FamilyHandle::from_semantic_family)
    }

    #[pyo3(name = "lineSources")]
    fn line_sources(&self) -> PyResult<Vec<String>> {
        self.0
            .line_sources()
            .map(|sources| sources.iter().map(ToString::to_string).collect())
            .map_err(engine_error)
    }

    #[pyo3(name = "lineNumberSources")]
    fn line_number_sources(&self) -> PyResult<Vec<String>> {
        self.0
            .line_number_sources()
            .map(|sources| sources.iter().map(ToString::to_string).collect())
            .map_err(engine_error)
    }
}

#[pyfunction]
#[pyo3(signature = (store, source, options, context))]
fn create_code(
    store: &Store,
    source: &str,
    options: &CodeOptions,
    context: Option<&mut Context>,
) -> PyResult<CodeHandle> {
    let options = options.0.clone();
    match context {
        Some(context) => {
            context.available()?;
            noon::Code::new(&mut context.scene, source, options)
        }
        None => noon::Code::in_store(Rc::clone(&store.semantics), source, options),
    }
    .map(CodeHandle)
    .map_err(engine_error)
}

#[pyfunction]
fn code_from_family(family: &FamilyHandle) -> PyResult<CodeHandle> {
    noon::Code::from_family(family.family.clone())
        .map(CodeHandle)
        .map_err(engine_error)
}

#[pyfunction]
fn code_styles_list() -> Vec<String> {
    noon::Code::get_styles_list()
}

#[pyfunction]
fn code_languages_list() -> Vec<String> {
    noon::Code::get_languages_list()
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<CodeOptions>()?;
    module.add_class::<CodeHandle>()?;
    module.add_function(wrap_pyfunction!(create_code, module)?)?;
    module.add_function(wrap_pyfunction!(code_from_family, module)?)?;
    module.add_function(wrap_pyfunction!(code_styles_list, module)?)?;
    module.add_function(wrap_pyfunction!(code_languages_list, module)?)
}
