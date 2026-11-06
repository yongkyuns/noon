//! Optional CPython bindings. Engine state and logical time stay in Noon Rust.
//! Objects are owner-thread confined, matching the existing Rc-based Rust API.
// Workspace `cargo test --lib` forces a harness even with lib.test=false.
// Real extension coverage runs through CPython, without embedding libpython in a Rust test binary.
#![cfg(all(not(target_arch = "wasm32"), not(test)))]
use noon::integration::AuthoringFailure;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
mod callback;
mod callback_values;
mod composition;
mod context;
mod coordinates;
mod effects;
mod family;
mod geometry;
mod image;
mod mobject;
mod options;
mod paints;
mod path_input;
mod path_query;
mod plotting;
mod svg;
#[cfg(feature = "native-text")]
mod text;
mod vector_path;
mod viewer;

fn engine_error(error: impl Into<AuthoringFailure>) -> PyErr {
    fn attach(py: Python<'_>, error: AuthoringFailure) -> PyErr {
        let exception = PyRuntimeError::new_err(error.message.clone());
        let value = exception.value(py);
        // Newly allocated exceptions have writable instance dictionaries.
        for (key, val) in [
            ("category", error.category),
            ("code", error.code),
            ("message", &error.message),
        ] {
            if let Err(failure) = value.setattr(key, val) {
                return failure;
            }
        }
        if let Err(failure) = value.setattr("noonErrorVersion", 1) {
            return failure;
        }
        if let Some(cause) = error.cause {
            let cause = attach(py, *cause);
            if let Err(failure) = value.setattr("cause", cause.value(py)) {
                return failure;
            }
        }
        exception
    }
    Python::attach(|py| attach(py, error.into()))
}

#[pymodule]
fn _noon_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<viewer::PreparedScene>()?;
    m.add_class::<image::ImageOptions>()?;
    m.add_class::<svg::SvgDefaults>()?;
    #[cfg(feature = "native-text")]
    text::register(m)?;
    m.add_class::<effects::EffectUpdate>()?;
    m.add_class::<effects::EffectDefinition>()?;
    m.add_class::<effects::EffectHandle>()?;
    m.add_class::<geometry::GeometryOptions>()?;
    m.add_class::<vector_path::AuthoringVectorPath>()?;
    m.add_class::<path_query::PathQuery>()?;
    m.add_class::<context::Store>()?;
    m.add_class::<context::Context>()?;
    m.add_class::<context::MembershipBatch>()?;
    m.add_class::<family::FamilyHandle>()?;
    m.add_class::<family::FamilyIterator>()?;
    m.add_class::<family::FamilySlicePlan>()?;
    m.add_class::<family::FamilyCopy>()?;
    m.add_class::<family::FamilyLayoutObservation>()?;
    m.add_class::<family::ColorObservation>()?;
    m.add_class::<mobject::MobjectHandle>()?;
    m.add_class::<mobject::LayoutAnchor>()?;
    m.add_class::<mobject::LayoutObservation>()?;
    m.add_class::<paints::ImageMapping>()?;
    m.add_class::<paints::ImagePaintOptions>()?;
    m.add_class::<paints::LinearGradient>()?;
    m.add_class::<paints::RadialGradient>()?;
    m.add_class::<paints::NoisePaint>()?;
    m.add_class::<paints::HatchPaint>()?;
    m.add_class::<composition::Composition>()?;
    m.add_class::<coordinates::CoordinateOptions>()?;
    #[cfg(feature = "native-text")]
    m.add_class::<coordinates::NumberLabelOptions>()?;
    m.add_class::<coordinates::NumberLineObservation>()?;
    m.add_class::<coordinates::AxesObservation>()?;
    m.add_class::<coordinates::PolarObservation>()?;
    m.add_class::<coordinates::ThreeDAxesObservation>()?;
    m.add_class::<plotting::PlotSamplingPlan>()?;
    m.add_function(wrap_pyfunction!(options::resolve_animation_options, m)?)?;
    m.add_function(wrap_pyfunction!(options::resolve_create_options, m)?)?;
    m.add_function(wrap_pyfunction!(options::resolve_glow_pulse_options, m)?)?;
    m.add_function(wrap_pyfunction!(options::resolve_transform_options, m)?)?;
    Ok(())
}
