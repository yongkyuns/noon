//! Optional CPython bindings. Engine state and logical time stay in Noon Rust.
//! Objects are owner-thread confined, matching the existing Rc-based Rust API.
#![cfg(not(target_arch = "wasm32"))]
use noon::integration::AuthoringFailure;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
mod callback;
mod callback_values;
mod composition;
mod context;
#[cfg(feature = "export")]
mod export;
mod geometry;
mod mobject;
mod options;

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
    #[cfg(feature = "export")]
    m.add_class::<export::VideoExport>()?;
    m.add_class::<geometry::GeometryOptions>()?;
    m.add_class::<context::Store>()?;
    m.add_class::<context::Context>()?;
    m.add_class::<context::MembershipBatch>()?;
    m.add_class::<mobject::MobjectHandle>()?;
    m.add_class::<mobject::LayoutAnchor>()?;
    m.add_class::<mobject::LayoutObservation>()?;
    m.add_class::<composition::Composition>()?;
    m.add_function(wrap_pyfunction!(options::resolve_animation_options, m)?)?;
    m.add_function(wrap_pyfunction!(options::resolve_transform_options, m)?)?;
    Ok(())
}
