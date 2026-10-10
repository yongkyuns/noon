//! Common optional-language argument projection before shared option resolution.
use noon_core::{
    resolve_animation_options as resolve_core_animation_options,
    resolve_transform_animation_options as resolve_core_transform_animation_options,
    AnimationDefaults, AnimationOptions, RateFunction, ResolvedAnimationOptions,
};

use crate::binding_error::AuthoringFailure;

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
    animation_run_time: f64,
    animation_rate_func: &str,
    animation_lag_ratio: f64,
    animation_path_arc: f64,
    animation_reverse_rate_function: i32,
    play_run_time: f64,
    play_rate_func: &str,
    play_lag_ratio: f64,
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    let animation_rate_func = parse_optional_rate_func(animation_rate_func)?;
    let play_rate_func = parse_optional_rate_func(play_rate_func)?;

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

    resolve_core_animation_options(
        AnimationDefaults::MANIM.lag_ratio(default_lag_ratio),
        animation,
        play,
    )
    .map_err(AuthoringFailure::from)
}

#[allow(clippy::too_many_arguments)]
pub fn resolve_frontend_transform_animation_options(
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
) -> Result<ResolvedAnimationOptions, AuthoringFailure> {
    let animation_rate_func = parse_optional_rate_func(animation_rate_func)?;
    let play_rate_func = parse_optional_rate_func(play_rate_func)?;
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
        path_arc: optional_number(play_path_arc),
        ..AnimationOptions::new()
    };
    resolve_core_transform_animation_options(
        AnimationDefaults::MANIM.lag_ratio(default_lag_ratio),
        animation,
        play,
    )
    .map_err(AuthoringFailure::from)
}
