//! Optional language coercion into the shared immutable Rust paint value.
use crate::AuthoringFailure;
use noon::{Color, GradientStop, LinearGradient, RadialGradient};
use std::sync::Arc;

fn linear_gradient(
    start: &[f64],
    end: &[f64],
    stops: &[f64],
) -> Result<Arc<LinearGradient>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    let start: [f64; 2] = start
        .try_into()
        .map_err(|_| invalid("gradient start requires two coordinates"))?;
    let end: [f64; 2] = end
        .try_into()
        .map_err(|_| invalid("gradient end requires two coordinates"))?;
    let stops = gradient_stops(stops)?;
    LinearGradient::new(start, end, stops)
        .map(Arc::new)
        .map_err(|error| invalid(&error.to_string()))
}

fn gradient_stops(stops: &[f64]) -> Result<Vec<GradientStop>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    if !stops.len().is_multiple_of(5) {
        return Err(invalid(
            "gradient stops require position and four color channels",
        ));
    }
    stops
        .as_chunks::<5>()
        .0
        .iter()
        .map(|stop| {
            if !stop[1..]
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
            {
                return Err(invalid(
                    "gradient colors require finite RGBA channels in [0, 1]",
                ));
            }
            GradientStop::new(
                stop[0],
                Color::rgba(
                    stop[1] as f32,
                    stop[2] as f32,
                    stop[3] as f32,
                    stop[4] as f32,
                ),
            )
            .map_err(|error| invalid(&error.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()
}

fn radial_gradient(
    center: &[f64],
    radii: &[f64],
    stops: &[f64],
) -> Result<Arc<RadialGradient>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_gradient", message);
    let center: [f64; 2] = center
        .try_into()
        .map_err(|_| invalid("radial center requires two coordinates"))?;
    let radii: [f64; 2] = radii
        .try_into()
        .map_err(|_| invalid("radial radii require two lengths"))?;
    RadialGradient::new(center, radii, gradient_stops(stops)?)
        .map(Arc::new)
        .map_err(|error| invalid(&error.to_string()))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use crate::{authoring_error::js_error, WasmAuthoringMobjectHandle};
    use wasm_bindgen::prelude::*;

    /// Immutable paint value; construction allocates no scene or GPU identity.
    #[wasm_bindgen]
    pub struct WasmLinearGradient {
        pub(crate) value: Arc<LinearGradient>,
    }

    #[wasm_bindgen]
    impl WasmLinearGradient {
        #[wasm_bindgen(constructor)]
        pub fn new(
            start: &[f64],
            end: &[f64],
            stops: &[f64],
        ) -> Result<WasmLinearGradient, JsValue> {
            linear_gradient(start, end, stops)
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    pub struct WasmRadialGradient {
        pub(crate) value: Arc<RadialGradient>,
    }

    #[wasm_bindgen]
    impl WasmRadialGradient {
        #[wasm_bindgen(constructor)]
        pub fn new(
            center: &[f64],
            radii: &[f64],
            stops: &[f64],
        ) -> Result<WasmRadialGradient, JsValue> {
            radial_gradient(center, radii, stops)
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setFillRadialGradient)]
        pub fn set_fill_radial_gradient(
            &self,
            gradient: &WasmRadialGradient,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_fill_radial_gradient(gradient.value.clone(), opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillGradient)]
        pub fn set_fill_gradient(
            &self,
            gradient: &WasmLinearGradient,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_fill_gradient(gradient.value.clone(), opacity)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_checks_shape_and_f64_channels_before_narrowing() {
        let stops = [0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0];
        let paint = linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &stops).unwrap();
        assert_eq!(
            paint.sample_premultiplied([0.5, 0.5]).unwrap(),
            [0.5, 0.0, 0.5, 1.0]
        );
        assert!(linear_gradient(&[0.0], &[1.0, 0.5], &stops).is_err());
        assert!(linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &stops[..9]).is_err());
        let mut invalid = stops;
        invalid[1] = 1.0 + f64::EPSILON;
        assert!(linear_gradient(&[0.0, 0.5], &[1.0, 0.5], &invalid).is_err());
        assert!(radial_gradient(&[0.5], &[0.5; 2], &stops).is_err());
        assert!(radial_gradient(&[0.5; 2], &[0.0, 0.5], &stops).is_err());
        assert!(radial_gradient(&[0.5; 2], &[0.5; 2], &invalid).is_err());
        let radial = radial_gradient(&[0.5; 2], &[0.5; 2], &stops).unwrap();
        assert_eq!(
            radial.sample_premultiplied([0.75, 0.5]).unwrap(),
            [0.5, 0.0, 0.5, 1.0]
        );
    }
}
