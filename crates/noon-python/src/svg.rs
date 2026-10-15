//! Cold SVG host inputs. Shared Rust owns parsing, inheritance and publication.
use crate::{context::Store, engine_error, family::FamilyHandle};
use pyo3::prelude::*;
use std::rc::Rc;

#[pyclass(frozen, module = "_noon_native")]
pub struct SvgDefaults {
    pub(crate) width: Option<f64>,
    pub(crate) style: noon::SvgDefaultStyle,
}

fn color(value: Option<u32>) -> PyResult<Option<noon::Color>> {
    value
        .map(|value| {
            if value > 0xFF_FFFF {
                Err(engine_error(noon::integration::AuthoringFailure::new(
                    "invalid_input",
                    "svg.invalid_default_transport",
                    "SVG default color must be a 24-bit RGB integer",
                )))
            } else {
                Ok(noon::Color::from_hex(value))
            }
        })
        .transpose()
}

pub(crate) fn defaults(
    width: Option<f64>,
    fill_color: Option<u32>,
    fill_opacity: Option<f64>,
    stroke_color: Option<u32>,
    stroke_opacity: Option<f64>,
    stroke_width: Option<f64>,
) -> PyResult<SvgDefaults> {
    Ok(SvgDefaults {
        width,
        style: noon::SvgDefaultStyle {
            color: None,
            opacity: None,
            fill_color: color(fill_color)?,
            fill_opacity,
            stroke_color: color(stroke_color)?,
            stroke_opacity,
            stroke_width,
        },
    })
}

pub(crate) fn request(
    should_center: bool,
    height: Option<f64>,
    width_or_defaults: &Bound<'_, PyAny>,
) -> PyResult<(noon::SvgImportOptions, Option<noon::SvgDefaultStyle>)> {
    let (width, style) = if let Ok(defaults) = width_or_defaults.extract::<PyRef<'_, SvgDefaults>>()
    {
        (defaults.width, Some(defaults.style))
    } else {
        (width_or_defaults.extract::<Option<f64>>()?, None)
    };
    let options = noon::SvgImportOptions {
        should_center,
        height,
        width,
    };
    Ok((options, style))
}

pub(crate) fn authored(
    store: &Store,
    source: &str,
    should_center: bool,
    height: Option<f64>,
    width_or_defaults: &Bound<'_, PyAny>,
) -> PyResult<FamilyHandle> {
    let (options, style) = request(should_center, height, width_or_defaults)?;
    let family = match style {
        Some(style) => noon::MobjectFamily::from_svg_str_with_options_and_svg_default(
            Rc::clone(&store.semantics),
            source,
            options,
            style,
        ),
        None => noon::MobjectFamily::from_svg_str_with_options(
            Rc::clone(&store.semantics),
            source,
            options,
        ),
    }
    .map_err(engine_error)?;
    Ok(FamilyHandle { family })
}
