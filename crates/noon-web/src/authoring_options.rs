pub use noon::integration::{
    resolve_frontend_animation_options, resolve_frontend_transform_animation_options,
};
#[cfg(target_arch = "wasm32")]
use noon_core::ResolvedAnimationOptions;
#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        resolve_frontend_animation_options, resolve_frontend_transform_animation_options,
        ResolvedAnimationOptions,
    };
    use crate::authoring_error::js_error;

    #[wasm_bindgen]
    pub struct WasmAnimationOptionsResolution(ResolvedAnimationOptions);

    #[wasm_bindgen]
    impl WasmAnimationOptionsResolution {
        #[wasm_bindgen(getter, js_name = runTime)]
        pub fn run_time(&self) -> f64 {
            self.0.run_time
        }

        #[wasm_bindgen(getter, js_name = rateFunc)]
        pub fn rate_func(&self) -> String {
            self.0.rate_func.semantic_id().to_owned()
        }

        #[wasm_bindgen(getter, js_name = lagRatio)]
        pub fn lag_ratio(&self) -> f64 {
            self.0.lag_ratio
        }

        #[wasm_bindgen(getter, js_name = pathArc)]
        pub fn path_arc(&self) -> f64 {
            self.0.path_arc
        }

        #[wasm_bindgen(getter, js_name = reverseRateFunction)]
        pub fn reverse_rate_function(&self) -> bool {
            self.0.reverse_rate_function
        }
    }

    #[wasm_bindgen(js_name = resolveAnimationOptions)]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_animation_options(
        default_lag_ratio: f64,
        animation_run_time: f64,
        animation_rate_func: &str,
        animation_lag_ratio: f64,
        animation_path_arc: f64,
        animation_reverse_rate_function: i32,
        play_run_time: f64,
        play_rate_func: &str,
        play_lag_ratio: f64,
    ) -> Result<WasmAnimationOptionsResolution, JsValue> {
        resolve_frontend_animation_options(
            default_lag_ratio,
            animation_run_time,
            animation_rate_func,
            animation_lag_ratio,
            animation_path_arc,
            animation_reverse_rate_function,
            play_run_time,
            play_rate_func,
            play_lag_ratio,
        )
        .map(WasmAnimationOptionsResolution)
        .map_err(js_error)
    }

    #[wasm_bindgen(js_name = resolveTransformAnimationOptions)]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_transform_animation_options(
        default_lag_ratio: f64,
        animation_run_time: f64,
        animation_rate_func: &str,
        animation_lag_ratio: f64,
        animation_path_arc: f64,
        animation_reverse_rate_function: i32,
        play_run_time: f64,
        play_rate_func: &str,
        play_lag_ratio: f64,
        play_path_arc: f64,
    ) -> Result<WasmAnimationOptionsResolution, JsValue> {
        resolve_frontend_transform_animation_options(
            default_lag_ratio,
            animation_run_time,
            animation_rate_func,
            animation_lag_ratio,
            animation_path_arc,
            animation_reverse_rate_function,
            play_run_time,
            play_rate_func,
            play_lag_ratio,
            play_path_arc,
        )
        .map(WasmAnimationOptionsResolution)
        .map_err(js_error)
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_bridge_uses_shared_precedence() {
        let resolved = resolve_frontend_animation_options(
            0.0,
            3.0,
            "linear",
            0.5,
            f64::NAN,
            -1,
            0.4,
            "smooth",
            f64::NAN,
        );

        let resolved = resolved.unwrap();
        assert_eq!(resolved.run_time, 0.4);
        assert_eq!(resolved.rate_func.semantic_id(), "smooth");
        assert_eq!(resolved.lag_ratio, 0.5);
    }

    #[test]
    fn transform_frontend_bridge_accepts_and_overrides_path_arc() {
        let resolved = resolve_frontend_transform_animation_options(
            0.0,
            f64::NAN,
            "",
            f64::NAN,
            0.5,
            -1,
            f64::NAN,
            "",
            f64::NAN,
            -0.75,
        )
        .unwrap();
        assert_eq!(resolved.path_arc, -0.75);
    }

    #[test]
    fn frontend_bridge_preserves_shared_error_policy() {
        let unsupported = resolve_frontend_animation_options(
            0.0,
            f64::NAN,
            "",
            f64::NAN,
            0.25,
            -1,
            f64::NAN,
            "",
            f64::NAN,
        );
        let error = unsupported.unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("unsupported_operation", "animation.unsupported_path_arc")
        );

        let invalid = resolve_frontend_animation_options(
            0.0,
            f64::NAN,
            "unknown",
            f64::NAN,
            f64::NAN,
            -1,
            f64::NAN,
            "",
            f64::NAN,
        );
        let error = invalid.unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("invalid_input", "animation.invalid_rate_function")
        );
    }
}
