//! Derived native callback values; operations remain shared Rust.
use crate::engine_error;
use noon::{Color, Style, Transform2D};
use pyo3::prelude::*;
pub(crate) fn callback_color(
    label: &str,
    red: Option<f64>,
    green: Option<f64>,
    blue: Option<f64>,
    alpha: Option<f64>,
) -> Result<Option<Color>, PyErr> {
    match (red, green, blue, alpha) {
        (None, None, None, None) => Ok(None),
        (Some(red), Some(green), Some(blue), Some(alpha)) => {
            let channel = |name: &str, value: f64| {
                if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
                    Err(engine_error(format!(
                        "{label}.{name} must be a finite f32-compatible number"
                    )))
                } else {
                    Ok(value as f32)
                }
            };
            if !(0.0..=1.0).contains(&alpha) {
                return Err(engine_error(format!(
                    "{label}.alpha must be between 0 and 1"
                )));
            }
            Ok(Some(Color::rgba(
                channel("red", red)?,
                channel("green", green)?,
                channel("blue", blue)?,
                alpha as f32,
            )))
        }
        _ => Err(engine_error(format!(
            "{label} must provide either all RGBA channels or none"
        ))),
    }
}
pub(crate) fn callback_paint_style(fill: Option<Color>, stroke: Option<Color>) -> Style {
    Style {
        fill,
        stroke,
        ..Style::default()
    }
}
pub(crate) fn callback_paint_result(style: Style) -> CallbackPaint {
    CallbackPaint {
        fill: style.fill,
        stroke: style.stroke,
    }
}

#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct CallbackPaint {
    fill: Option<Color>,
    stroke: Option<Color>,
}
#[pymethods]
impl CallbackPaint {
    #[getter(hasFill)]
    fn has_fill(&self) -> bool {
        self.fill.is_some()
    }
    #[getter(fillRed)]
    fn fill_red(&self) -> f64 {
        self.fill.map_or(0.0, |c| f64::from(c.red))
    }
    #[getter(fillGreen)]
    fn fill_green(&self) -> f64 {
        self.fill.map_or(0.0, |c| f64::from(c.green))
    }
    #[getter(fillBlue)]
    fn fill_blue(&self) -> f64 {
        self.fill.map_or(0.0, |c| f64::from(c.blue))
    }
    #[getter(fillAlpha)]
    fn fill_alpha(&self) -> f64 {
        self.fill.map_or(0.0, |c| f64::from(c.alpha))
    }
    #[getter(hasStroke)]
    fn has_stroke(&self) -> bool {
        self.stroke.is_some()
    }
    #[getter(strokeRed)]
    fn stroke_red(&self) -> f64 {
        self.stroke.map_or(0.0, |c| f64::from(c.red))
    }
    #[getter(strokeGreen)]
    fn stroke_green(&self) -> f64 {
        self.stroke.map_or(0.0, |c| f64::from(c.green))
    }
    #[getter(strokeBlue)]
    fn stroke_blue(&self) -> f64 {
        self.stroke.map_or(0.0, |c| f64::from(c.blue))
    }
    #[getter(strokeAlpha)]
    fn stroke_alpha(&self) -> f64 {
        self.stroke.map_or(0.0, |c| f64::from(c.alpha))
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
pub(crate) struct CallbackTransform {
    pub(crate) transform: Transform2D,
}
#[pymethods]
impl CallbackTransform {
    #[getter(translationX)]
    fn translationx(&self) -> f64 {
        f64::from(self.transform.translation.x)
    }
    #[getter(translationY)]
    fn translationy(&self) -> f64 {
        f64::from(self.transform.translation.y)
    }
    #[getter(scaleX)]
    fn scalex(&self) -> f64 {
        f64::from(self.transform.scale.x)
    }
    #[getter(scaleY)]
    fn scaley(&self) -> f64 {
        f64::from(self.transform.scale.y)
    }
    #[getter(rotation)]
    fn rotation(&self) -> f64 {
        f64::from(self.transform.rotation)
    }
}
