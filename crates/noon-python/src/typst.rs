//! Optional Typst projections to the existing Rust resource compiler and live admission.
use crate::{
    context::{Context, Store},
    engine_error,
    mobject::MobjectHandle,
};
use pyo3::prelude::*;
use std::rc::Rc;

fn font_size(value: f64) -> PyResult<f32> {
    noon::integration::authoring_render_f64("font size", value)
        .map(|v| v as f32)
        .map_err(engine_error)
}
pub(crate) fn authored(
    store: &Store,
    source: &str,
    math: bool,
    size: f64,
) -> PyResult<MobjectHandle> {
    let size = font_size(size)?;
    let result = if math {
        noon::Mobject::from_math_typst(
            Rc::clone(&store.semantics),
            noon::MathTypst::new(source).with_font_size(size),
        )
    } else {
        noon::Mobject::from_typst(
            Rc::clone(&store.semantics),
            noon::Typst::new(source).with_font_size(size),
        )
    };
    result
        .map(MobjectHandle::from_semantic_mobject)
        .map_err(engine_error)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn live(
    context: &mut Context,
    source: &str,
    math: bool,
    size: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
    opacity: f64,
) -> PyResult<MobjectHandle> {
    context.available()?;
    let size = font_size(size)?;
    let channel = |name, value| {
        noon::integration::authoring_render_f64(name, value)
            .map(|v| v as f32)
            .map_err(engine_error)
    };
    let color = noon::Color::rgba(
        channel("text red", red)?,
        channel("text green", green)?,
        channel("text blue", blue)?,
        channel("text alpha", alpha)?,
    );
    let opacity = channel("text opacity", opacity)?;
    let running = context.has_execution();
    let result = if math {
        context.scene.math_typst(
            noon::MathTypst::new(source)
                .with_font_size(size)
                .color(color)
                .set_opacity(opacity),
        )
    } else {
        context.scene.typst(
            noon::Typst::new(source)
                .with_font_size(size)
                .color(color)
                .set_opacity(opacity),
        )
    };
    result
        .map(MobjectHandle::from_semantic_mobject)
        .map_err(|error| {
            if running {
                engine_error(noon::integration::live_text_failure(error))
            } else {
                engine_error(error)
            }
        })
}
