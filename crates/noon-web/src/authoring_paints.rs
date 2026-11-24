//! Optional language coercion into the shared immutable Rust paint value.
#[cfg(any(test, target_arch = "wasm32"))]
use noon::integration::{hatch_paint, linear_gradient, noise_paint, radial_gradient};
#[cfg(test)]
use noon::Color;
#[cfg(target_arch = "wasm32")]
use noon::{HatchPaint, LinearGradient, NoisePaint, RadialGradient};
#[cfg(target_arch = "wasm32")]
use std::sync::Arc;

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
    pub struct WasmHatchPaint {
        pub(crate) value: Arc<HatchPaint>,
    }

    #[wasm_bindgen]
    impl WasmHatchPaint {
        #[wasm_bindgen(constructor)]
        pub fn new(
            spacing: f64,
            width: f64,
            angle: f64,
            phase: &[f64],
            cross: bool,
            colors: &[f64],
        ) -> Result<WasmHatchPaint, JsValue> {
            hatch_paint(spacing, width, angle, phase, cross, colors)
                .map(|value| Self { value })
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = setFillHatch)]
        pub fn set_fill_hatch(
            &self,
            paint: &WasmHatchPaint,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            self.semantic_mobject()
                .clone()
                .set_fill_hatch(paint.value.clone(), opacity)
                .map_err(js_error)
        }

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
    fn hatch_boundary_preserves_units_and_checks_channels_before_narrowing() {
        let colors = [1.0, 0.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.25];
        let value = hatch_paint(0.4, 0.08, 0.7, &[-0.173, 3.231], true, &colors).unwrap();
        assert_eq!(value.spacing(), 0.4);
        assert_eq!(value.width(), 0.08);
        assert_eq!(value.angle(), 0.7);
        assert_eq!(value.phase(), [-0.173, 3.231]);
        assert!(value.cross());
        assert_eq!(
            value.colors(),
            [
                Color::rgba(0.0, 0.0, 1.0, 0.25),
                Color::rgba(1.0, 0.0, 0.0, 0.5)
            ]
        );
        assert!(hatch_paint(0.4, 0.08, 0.7, &[0.0], false, &colors).is_err());
        let mut invalid = colors;
        invalid[0] = 1.0 + f64::EPSILON;
        assert!(hatch_paint(0.4, 0.08, 0.7, &[0.0; 2], false, &invalid).is_err());
        for (spacing, width, angle) in [(0.0, 0.0, 0.0), (0.4, 0.5, 0.0), (0.4, 0.08, f64::NAN)] {
            assert!(hatch_paint(spacing, width, angle, &[0.0; 2], false, &colors).is_err());
        }
    }

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
