//! Native TeX admission through the existing system compiler and Scene.
use crate::{
    context::{Context, Store},
    engine_error,
    family::FamilyHandle,
    mobject::MobjectHandle,
};
use noon::integration::{latex_member_colors, AuthoringFailure, LatexInput};
use noon_native::{NativeLatexBackend, NativeLatexConfig};
use pyo3::prelude::*;
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct LatexCompiler(Rc<RefCell<Option<NativeLatexBackend>>>);

impl LatexCompiler {
    pub(crate) fn with_backend<T>(
        &self,
        compile: impl FnOnce(&mut NativeLatexBackend) -> PyResult<T>,
    ) -> PyResult<T> {
        let mut slot = self.0.borrow_mut();
        if slot.is_none() {
            *slot = Some(system_backend()?);
        }
        compile(slot.as_mut().expect("compiler initialized"))
    }
}

fn system_backend() -> PyResult<NativeLatexBackend> {
    let configuration = |message| {
        engine_error(AuthoringFailure::new(
            "unsupported_operation",
            "text.native_latex_configuration",
            message,
        ))
    };
    let fonts = std::env::var_os("NOON_LATEX_FONT_DIR")
        .ok_or_else(|| configuration("Set NOON_LATEX_FONT_DIR to the BaKoMa TTF directory"))?;
    let identity = std::env::var("NOON_LATEX_RESOURCE_IDENTITY").map_err(|_| {
        configuration("Set NOON_LATEX_RESOURCE_IDENTITY to identify the compiler and fonts")
    })?;
    let executable = |variable, name: &str| -> PathBuf {
        if let Some(path) = std::env::var_os(variable) {
            return path.into();
        }
        let macos_path = PathBuf::from("/Library/TeX/texbin").join(name);
        if macos_path.is_file() {
            macos_path
        } else {
            name.into()
        }
    };
    NativeLatexBackend::new(NativeLatexConfig::new(
        executable("NOON_LATEX_EXECUTABLE", "latex"),
        executable("NOON_KPSEWHICH_EXECUTABLE", "kpsewhich"),
        fonts,
        identity,
    ))
    .map_err(|error| {
        engine_error(AuthoringFailure::new(
            "unsupported_operation",
            "text.native_latex_configuration",
            error,
        ))
    })
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct LatexHandle(pub(crate) noon::LatexParts);

#[pymethods]
impl LatexHandle {
    fn family(&self) -> FamilyHandle {
        FamilyHandle::from_semantic_family(self.0.family().clone())
    }

    #[pyo3(name = "rebindFamily")]
    fn rebind_family(&self, family: &FamilyHandle) -> PyResult<Self> {
        self.0
            .rebind_family(family.family.clone())
            .map(Self)
            .map_err(engine_error)
    }

    fn members(&self) -> PyResult<Vec<MobjectHandle>> {
        self.0
            .current_members()
            .map(|members| {
                members
                    .into_iter()
                    .map(MobjectHandle::from_semantic_mobject)
                    .collect()
            })
            .map_err(engine_error)
    }

    #[pyo3(name = "sourceMemberIndicesFor")]
    fn source_member_indices_for(&self, needle: &str) -> PyResult<Vec<usize>> {
        self.0
            .current_member_indices_for(needle)
            .map_err(engine_error)
    }

    #[pyo3(name = "setMemberColors")]
    fn set_member_colors(&self, rgba: Vec<f64>) -> PyResult<()> {
        let colors = latex_member_colors(&rgba).map_err(engine_error)?;
        self.0
            .set_current_member_colors(&colors)
            .map_err(engine_error)
    }

    #[getter]
    fn source(&self) -> &str {
        self.0.source()
    }

    #[getter(fontSize)]
    fn font_size(&self) -> PyResult<f64> {
        self.0.current_font_size().map_err(engine_error)
    }
}

#[pyfunction]
fn prepare_latex(store: &Store) -> PyResult<()> {
    store.latex.with_backend(|_| Ok(()))
}

#[pyfunction]
#[pyo3(signature = (store, strings, math, size, color, opacity, isolates, context))]
#[allow(clippy::too_many_arguments)]
fn create_latex(
    store: &Store,
    strings: Vec<String>,
    math: bool,
    size: f64,
    color: [f64; 4],
    opacity: f64,
    isolates: Vec<String>,
    context: Option<&mut Context>,
) -> PyResult<LatexHandle> {
    let input = match strings.as_slice() {
        [source] => LatexInput::new(source, math, size, &color, opacity),
        _ => LatexInput::from_strings(strings, math, size, &color, opacity),
    }
    .map_err(engine_error)?
    .with_isolated_substrings(&isolates)
    .map_err(engine_error)?;
    store
        .latex
        .with_backend(|backend| match context {
            Some(context) => {
                context.available()?;
                let running = context.has_execution();
                input.publish(&mut context.scene, backend).map_err(|error| {
                    if running {
                        engine_error(noon::integration::live_text_failure(error))
                    } else {
                        engine_error(error)
                    }
                })
            }
            None => input
                .publish_detached(Rc::clone(&store.semantics), backend)
                .map_err(engine_error),
        })
        .map(LatexHandle)
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<LatexHandle>()?;
    module.add_function(wrap_pyfunction!(prepare_latex, module)?)?;
    module.add_function(wrap_pyfunction!(create_latex, module)?)
}
