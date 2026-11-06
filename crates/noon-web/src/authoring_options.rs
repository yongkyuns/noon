pub use noon::integration::{
    resolve_frontend_animation_options, resolve_frontend_create_animation_options,
    resolve_frontend_glow_pulse_animation_options, resolve_frontend_transform_animation_options,
};

#[cfg(target_arch = "wasm32")]
use noon_core::ResolvedAnimationOptions;

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        resolve_frontend_animation_options, resolve_frontend_create_animation_options,
        resolve_frontend_glow_pulse_animation_options,
        resolve_frontend_transform_animation_options, ResolvedAnimationOptions,
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
        animation_run_time: Option<f64>,
        animation_rate_func: &str,
        animation_lag_ratio: Option<f64>,
        animation_path_arc: Option<f64>,
        animation_reverse_rate_function: i32,
        play_run_time: Option<f64>,
        play_rate_func: &str,
        play_lag_ratio: Option<f64>,
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

    #[wasm_bindgen(js_name = resolveCreateAnimationOptions)]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_create_animation_options(
        default_lag_ratio: f64,
        animation_run_time: Option<f64>,
        animation_rate_func: &str,
        animation_lag_ratio: Option<f64>,
        animation_path_arc: Option<f64>,
        animation_reverse_rate_function: i32,
        play_run_time: Option<f64>,
        play_rate_func: &str,
        play_lag_ratio: Option<f64>,
    ) -> Result<WasmAnimationOptionsResolution, JsValue> {
        resolve_frontend_create_animation_options(
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

    #[wasm_bindgen(js_name = resolveGlowPulseAnimationOptions)]
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_glow_pulse_animation_options(
        default_lag_ratio: f64,
        animation_run_time: Option<f64>,
        animation_rate_func: &str,
        animation_lag_ratio: Option<f64>,
        animation_path_arc: Option<f64>,
        animation_reverse_rate_function: i32,
        play_run_time: Option<f64>,
        play_rate_func: &str,
        play_lag_ratio: Option<f64>,
    ) -> Result<WasmAnimationOptionsResolution, JsValue> {
        resolve_frontend_glow_pulse_animation_options(
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
        animation_run_time: Option<f64>,
        animation_rate_func: &str,
        animation_lag_ratio: Option<f64>,
        animation_path_arc: Option<f64>,
        animation_reverse_rate_function: i32,
        play_run_time: Option<f64>,
        play_rate_func: &str,
        play_lag_ratio: Option<f64>,
        play_path_arc: Option<f64>,
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
