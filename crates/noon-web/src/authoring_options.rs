use noon_core::{
    resolve_animation_options as resolve_core_animation_options,
    resolve_create_animation_options as resolve_core_create_animation_options,
    resolve_glow_pulse_animation_options as resolve_core_glow_pulse_animation_options,
    resolve_transform_animation_options as resolve_core_transform_animation_options,
    AnimationDefaults, AnimationOptions, RateFunction, ResolvedAnimationOptions,
};

use crate::authoring_error::AuthoringFailure;

fn optional_bool(value: i32) -> Option<bool> {
    match value {
        -1 => None,
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn parse_optional_rate_func(value: &str) -> Result<Option<RateFunction>, AuthoringFailure> {
    if value.is_empty() {
        return Ok(None);
    }
    RateFunction::from_semantic_id(value)
        .map(Some)
        .ok_or_else(|| {
            AuthoringFailure::new(
                "invalid_input",
                "animation.invalid_rate_function",
                format!("unsupported rate function semantic id: {value}"),
            )
        })
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_animation_options(
    default_lag_ratio: f64,
    animation_run_time: Option<f64>,
    animation_rate_func: &str,
    animation_lag_ratio: Option<f64>,
    animation_path_arc: Option<f64>,
    animation_reverse_rate_function: i32,
    play_run_time: Option<f64>,
    play_rate_func: &str,
    play_lag_ratio: Option<f64>,
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    resolve_frontend_options(
        resolve_core_animation_options,
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
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_create_animation_options(
    default_lag_ratio: f64,
    animation_run_time: Option<f64>,
    animation_rate_func: &str,
    animation_lag_ratio: Option<f64>,
    animation_path_arc: Option<f64>,
    animation_reverse_rate_function: i32,
    play_run_time: Option<f64>,
    play_rate_func: &str,
    play_lag_ratio: Option<f64>,
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    resolve_frontend_options(
        resolve_core_create_animation_options,
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
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_glow_pulse_animation_options(
    default_lag_ratio: f64,
    animation_run_time: Option<f64>,
    animation_rate_func: &str,
    animation_lag_ratio: Option<f64>,
    animation_path_arc: Option<f64>,
    animation_reverse_rate_function: i32,
    play_run_time: Option<f64>,
    play_rate_func: &str,
    play_lag_ratio: Option<f64>,
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    resolve_frontend_options(
        resolve_core_glow_pulse_animation_options,
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
}

#[allow(clippy::too_many_arguments)]
fn resolve_frontend_options(
    resolver: fn(
        AnimationDefaults,
        AnimationOptions,
        AnimationOptions,
    ) -> Result<ResolvedAnimationOptions, noon_core::AnimationOptionsError>,
    default_lag_ratio: f64,
    animation_run_time: Option<f64>,
    animation_rate_func: &str,
    animation_lag_ratio: Option<f64>,
    animation_path_arc: Option<f64>,
    animation_reverse_rate_function: i32,
    play_run_time: Option<f64>,
    play_rate_func: &str,
    play_lag_ratio: Option<f64>,
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    let animation_rate_func = parse_optional_rate_func(animation_rate_func)?;
    let play_rate_func = parse_optional_rate_func(play_rate_func)?;

    let animation = AnimationOptions {
        run_time: animation_run_time,
        rate_func: animation_rate_func,
        lag_ratio: animation_lag_ratio,
        path_arc: animation_path_arc,
        reverse_rate_function: optional_bool(animation_reverse_rate_function),
        ..AnimationOptions::new()
    };
    let play = AnimationOptions {
        run_time: play_run_time,
        rate_func: play_rate_func,
        lag_ratio: play_lag_ratio,
        ..AnimationOptions::new()
    };

    resolver(
        AnimationDefaults::MANIM.lag_ratio(default_lag_ratio),
        animation,
        play,
    )
    .map_err(AuthoringFailure::from)
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_transform_animation_options(
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
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    let animation_rate_func = parse_optional_rate_func(animation_rate_func)?;
    let play_rate_func = parse_optional_rate_func(play_rate_func)?;
    let animation = AnimationOptions {
        run_time: animation_run_time,
        rate_func: animation_rate_func,
        lag_ratio: animation_lag_ratio,
        path_arc: animation_path_arc,
        reverse_rate_function: optional_bool(animation_reverse_rate_function),
        ..AnimationOptions::new()
    };
    let play = AnimationOptions {
        run_time: play_run_time,
        rate_func: play_rate_func,
        lag_ratio: play_lag_ratio,
        path_arc: play_path_arc,
        ..AnimationOptions::new()
    };
    resolve_core_transform_animation_options(
        AnimationDefaults::MANIM.lag_ratio(default_lag_ratio),
        animation,
        play,
    )
    .map_err(AuthoringFailure::from)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_frontend_bridge_uses_create_policy_and_original_rate_identity() {
        let resolved = resolve_frontend_create_animation_options(
            0.0,
            Some(1.0),
            "lingering",
            None,
            None,
            1,
            Some(0.8),
            "",
            None,
        )
        .unwrap();
        assert!(resolved.reverse_rate_function);
        assert_eq!(resolved.rate_func, RateFunction::Lingering);
        assert_eq!(resolved.run_time, 0.8);
        let rejected = resolve_frontend_create_animation_options(
            0.0,
            Some(1.0),
            "lingering",
            None,
            Some(0.5),
            1,
            None,
            "",
            None,
        )
        .unwrap_err();
        assert_eq!(rejected.code, "animation.unsupported_path_arc");
    }

    #[test]
    fn frontend_bridge_uses_shared_precedence() {
        let resolved = resolve_frontend_animation_options(
            0.0,
            Some(3.0),
            "linear",
            Some(0.5),
            None,
            -1,
            Some(0.4),
            "smooth",
            None,
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
            None,
            "",
            None,
            Some(0.5),
            -1,
            None,
            "",
            None,
            Some(-0.75),
        )
        .unwrap();
        assert_eq!(resolved.path_arc, -0.75);
    }

    #[test]
    fn frontend_bridge_preserves_shared_error_policy() {
        let unsupported =
            resolve_frontend_animation_options(0.0, None, "", None, Some(0.25), -1, None, "", None);
        let error = unsupported.unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("unsupported_operation", "animation.unsupported_path_arc")
        );

        let invalid = resolve_frontend_animation_options(
            0.0, None, "unknown", None, None, -1, None, "", None,
        );
        let error = invalid.unwrap_err();
        assert_eq!(
            (error.category, error.code),
            ("invalid_input", "animation.invalid_rate_function")
        );
    }

    #[test]
    fn frontend_bridge_distinguishes_omission_from_invalid_duration() {
        let defaults =
            resolve_frontend_animation_options(0.0, None, "", None, None, -1, None, "", None)
                .unwrap();
        assert_eq!(defaults.run_time, 1.0);

        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -0.25] {
            for (animation, play) in [(Some(invalid), None), (None, Some(invalid))] {
                for result in [
                    resolve_frontend_animation_options(
                        0.0, animation, "", None, None, -1, play, "", None,
                    ),
                    resolve_frontend_create_animation_options(
                        0.0, animation, "", None, None, -1, play, "", None,
                    ),
                    resolve_frontend_glow_pulse_animation_options(
                        0.0, animation, "", None, None, -1, play, "", None,
                    ),
                    resolve_frontend_transform_animation_options(
                        0.0, animation, "", None, None, -1, play, "", None, None,
                    ),
                ] {
                    let error = result.unwrap_err();
                    assert_eq!(
                        (error.category, error.code),
                        ("invalid_input", "animation.invalid_run_time")
                    );
                }
            }
        }
    }

    #[test]
    fn frontend_bridge_preserves_explicit_nan_lag_and_path() {
        let lag = resolve_frontend_animation_options(
            0.0,
            None,
            "",
            Some(f64::NAN),
            None,
            -1,
            None,
            "",
            None,
        )
        .unwrap_err();
        assert_eq!(lag.code, "animation.invalid_lag_ratio");
        let path = resolve_frontend_transform_animation_options(
            0.0,
            None,
            "",
            None,
            Some(f64::NAN),
            -1,
            None,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(path.code, "animation.invalid_path_arc");
    }

    #[test]
    fn glow_pulse_frontend_bridge_preserves_reversal_and_play_overrides() {
        let resolved = resolve_frontend_glow_pulse_animation_options(
            0.0,
            Some(1.0),
            "there_and_back",
            None,
            None,
            1,
            Some(0.6),
            "linear",
            None,
        )
        .unwrap();
        assert_eq!(resolved.run_time, 0.6);
        assert_eq!(resolved.rate_func, RateFunction::Linear);
        assert!(resolved.reverse_rate_function);
        let path = resolve_frontend_glow_pulse_animation_options(
            0.0,
            None,
            "",
            None,
            Some(0.25),
            1,
            None,
            "",
            None,
        )
        .unwrap_err();
        assert_eq!(path.code, "animation.unsupported_path_arc");
    }
}
