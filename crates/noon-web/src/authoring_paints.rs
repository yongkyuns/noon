//! Optional language coercion into the shared immutable Rust paint value.
use crate::AuthoringFailure;
use noon::{Color, GradientStop, LinearGradient, NoisePaint, RadialGradient};
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

fn noise_paint(
    seed: f64,
    frequency: &[f64],
    phase: &[f64],
    colors: &[f64],
) -> Result<Arc<NoisePaint>, AuthoringFailure> {
    let invalid =
        |message: &str| AuthoringFailure::new("invalid_input", "paint.invalid_noise", message);
    if !seed.is_finite() || seed < 0.0 || seed > f64::from(u32::MAX) || seed.fract() != 0.0 {
        return Err(invalid("noise seed requires an integer in [0, 4294967295]"));
    }
    let frequency: [f64; 2] = frequency
        .try_into()
        .map_err(|_| invalid("noise frequency requires two cell counts"))?;
    let phase: [f64; 2] = phase
        .try_into()
        .map_err(|_| invalid("noise phase requires two coordinates"))?;
    if colors.len() != 8
        || colors
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid("noise requires two finite RGBA colors in [0, 1]"));
    }
    let colors = [
        Color::rgba(
            colors[0] as f32,
            colors[1] as f32,
            colors[2] as f32,
            colors[3] as f32,
        ),
        Color::rgba(
            colors[4] as f32,
            colors[5] as f32,
            colors[6] as f32,
            colors[7] as f32,
        ),
    ];
    NoisePaint::new(seed as u32, frequency, phase, colors)
        .map(Arc::new)
        .map_err(|e| invalid(&e.to_string()))
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
    pub struct WasmNoisePaint {
        pub(crate) value: Arc<NoisePaint>,
    }

    #[wasm_bindgen]
    impl WasmNoisePaint {
        #[wasm_bindgen(constructor)]
        pub fn new(
            seed: f64,
            frequency: &[f64],
            phase: &[f64],
            colors: &[f64],
        ) -> Result<WasmNoisePaint, JsValue> {
            noise_paint(seed, frequency, phase, colors)
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setFillNoise)]
        pub fn set_fill_noise(
            &self,
            paint: &WasmNoisePaint,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_fill_noise(paint.value.clone(), opacity)
                .map_err(js_error)
        }

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
    fn noise_boundary_preserves_seed_and_validates_f64_before_narrowing() {
        let colors = [1.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 1.0];
        let value = noise_paint(f64::from(u32::MAX), &[8.0; 2], &[-0.173, 0.231], &colors).unwrap();
        assert_eq!(value.seed(), u32::MAX);
        assert_eq!(value.phase(), [-0.173, 0.231]);
        for seed in [-1.0, 0.5, f64::NAN, 4294967296.0] {
            assert!(noise_paint(seed, &[8.0; 2], &[0.0; 2], &colors).is_err());
        }
        assert!(noise_paint(7.0, &[8.0], &[0.0; 2], &colors).is_err());
        let mut invalid = colors;
        invalid[0] = 1.0 + f64::EPSILON;
        assert!(noise_paint(7.0, &[8.0; 2], &[0.0; 2], &invalid).is_err());
    }
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
