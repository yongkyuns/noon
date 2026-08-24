use noon_core::{
    resolve_animation_options as resolve_core_animation_options, AnimationDefaults,
    AnimationOptions, AnimationOptionsError, RateFunction, ResolvedAnimationOptions,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendAnimationOptionsResolution {
    ok: bool,
    run_time: f64,
    rate_func: String,
    lag_ratio: f64,
    path_arc: f64,
    reverse_rate_function: bool,
    error_kind: Option<String>,
    message: Option<String>,
}

impl FrontendAnimationOptionsResolution {
    fn success(options: ResolvedAnimationOptions) -> Self {
        Self {
            ok: true,
            run_time: options.run_time,
            rate_func: options.rate_func.semantic_id().to_owned(),
            lag_ratio: options.lag_ratio,
            path_arc: options.path_arc,
            reverse_rate_function: options.reverse_rate_function,
            error_kind: None,
            message: None,
        }
    }

    fn failure(kind: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            run_time: 0.0,
            rate_func: String::new(),
            lag_ratio: 0.0,
            path_arc: 0.0,
            reverse_rate_function: false,
            error_kind: Some(kind.to_owned()),
            message: Some(message.into()),
        }
    }

    pub const fn ok(&self) -> bool {
        self.ok
    }

    pub const fn run_time(&self) -> f64 {
        self.run_time
    }

    pub fn rate_func(&self) -> String {
        self.rate_func.clone()
    }

    pub const fn lag_ratio(&self) -> f64 {
        self.lag_ratio
    }

    pub const fn path_arc(&self) -> f64 {
        self.path_arc
    }

    pub const fn reverse_rate_function(&self) -> bool {
        self.reverse_rate_function
    }

    pub fn error_kind(&self) -> Option<String> {
        self.error_kind.clone()
    }

    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

fn optional_number(value: f64) -> Option<f64> {
    (!value.is_nan()).then_some(value)
}

fn optional_bool(value: i32) -> Option<bool> {
    match value {
        -1 => None,
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn parse_optional_rate_func(value: &str) -> Result<Option<RateFunction>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    RateFunction::from_semantic_id(value)
        .map(Some)
        .ok_or_else(|| format!("unsupported rate function semantic id: {value}"))
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_animation_options(
    default_lag_ratio: f64,
    animation_run_time: f64,
    animation_rate_func: &str,
    animation_lag_ratio: f64,
    animation_path_arc: f64,
    animation_reverse_rate_function: i32,
    play_run_time: f64,
    play_rate_func: &str,
    play_lag_ratio: f64,
) -> FrontendAnimationOptionsResolution {
    let animation_rate_func = match parse_optional_rate_func(animation_rate_func) {
        Ok(value) => value,
        Err(message) => {
            return FrontendAnimationOptionsResolution::failure("invalid_rate_func", message)
        }
    };
    let play_rate_func = match parse_optional_rate_func(play_rate_func) {
        Ok(value) => value,
        Err(message) => {
            return FrontendAnimationOptionsResolution::failure("invalid_rate_func", message)
        }
    };

    let animation = AnimationOptions {
        run_time: optional_number(animation_run_time),
        rate_func: animation_rate_func,
        lag_ratio: optional_number(animation_lag_ratio),
        path_arc: optional_number(animation_path_arc),
        reverse_rate_function: optional_bool(animation_reverse_rate_function),
        ..AnimationOptions::new()
    };
    let play = AnimationOptions {
        run_time: optional_number(play_run_time),
        rate_func: play_rate_func,
        lag_ratio: optional_number(play_lag_ratio),
        ..AnimationOptions::new()
    };

    match resolve_core_animation_options(
        AnimationDefaults::MANIM.lag_ratio(default_lag_ratio),
        animation,
        play,
    ) {
        Ok(options) => FrontendAnimationOptionsResolution::success(options),
        Err(error) => {
            let kind = match error {
                AnimationOptionsError::UnsupportedPathArc(_)
                | AnimationOptionsError::UnsupportedReverseRateFunction => "unsupported",
                AnimationOptionsError::InvalidRunTime(_)
                | AnimationOptionsError::InvalidLagRatio(_)
                | AnimationOptionsError::InvalidPathArc(_) => "value_error",
            };
            FrontendAnimationOptionsResolution::failure(kind, error.to_string())
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{resolve_frontend_animation_options, FrontendAnimationOptionsResolution};

    #[wasm_bindgen]
    pub struct WasmAnimationOptionsResolution(FrontendAnimationOptionsResolution);

    #[wasm_bindgen]
    impl WasmAnimationOptionsResolution {
        #[wasm_bindgen(getter)]
        pub fn ok(&self) -> bool {
            self.0.ok()
        }

        #[wasm_bindgen(getter, js_name = runTime)]
        pub fn run_time(&self) -> f64 {
            self.0.run_time()
        }

        #[wasm_bindgen(getter, js_name = rateFunc)]
        pub fn rate_func(&self) -> String {
            self.0.rate_func()
        }

        #[wasm_bindgen(getter, js_name = lagRatio)]
        pub fn lag_ratio(&self) -> f64 {
            self.0.lag_ratio()
        }

        #[wasm_bindgen(getter, js_name = pathArc)]
        pub fn path_arc(&self) -> f64 {
            self.0.path_arc()
        }

        #[wasm_bindgen(getter, js_name = reverseRateFunction)]
        pub fn reverse_rate_function(&self) -> bool {
            self.0.reverse_rate_function()
        }

        #[wasm_bindgen(getter, js_name = errorKind)]
        pub fn error_kind(&self) -> Option<String> {
            self.0.error_kind()
        }

        #[wasm_bindgen(getter)]
        pub fn message(&self) -> Option<String> {
            self.0.message()
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
    ) -> WasmAnimationOptionsResolution {
        WasmAnimationOptionsResolution(resolve_frontend_animation_options(
            default_lag_ratio,
            animation_run_time,
            animation_rate_func,
            animation_lag_ratio,
            animation_path_arc,
            animation_reverse_rate_function,
            play_run_time,
            play_rate_func,
            play_lag_ratio,
        ))
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

        assert!(resolved.ok());
        assert_eq!(resolved.run_time(), 0.4);
        assert_eq!(resolved.rate_func(), "smooth");
        assert_eq!(resolved.lag_ratio(), 0.5);
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
        assert!(!unsupported.ok());
        assert_eq!(unsupported.error_kind().as_deref(), Some("unsupported"));

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
        assert!(!invalid.ok());
        assert_eq!(invalid.error_kind().as_deref(), Some("invalid_rate_func"));
    }
}
