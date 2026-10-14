//! Shared deterministic rate tracks, paired with ordinary_polynomial_rates.py.
use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};

pub fn session() -> Result<ExecutionSession, String> {
    build_session(
        &[
            RateFunction::Smoothstep,
            RateFunction::Smootherstep,
            RateFunction::Smoothererstep,
            RateFunction::SlowInto,
            RateFunction::DoubleSmooth,
        ],
        0.65,
        1.0,
    )
}

pub fn easing_session() -> Result<ExecutionSession, String> {
    build_session(
        &[
            RateFunction::EaseInQuad,
            RateFunction::EaseOutQuad,
            RateFunction::EaseInOutQuad,
            RateFunction::EaseInCubic,
            RateFunction::EaseOutCubic,
            RateFunction::EaseInOutCubic,
            RateFunction::EaseInQuart,
            RateFunction::EaseOutQuart,
            RateFunction::EaseInOutQuart,
            RateFunction::EaseInQuint,
            RateFunction::EaseOutQuint,
            RateFunction::EaseInOutQuint,
        ],
        0.28,
        0.38,
    )
}

/// Three standard sine curves through the same retained method-target composition.
pub fn sine_session() -> Result<ExecutionSession, String> {
    build_session(
        &[
            RateFunction::EaseInSine,
            RateFunction::EaseOutSine,
            RateFunction::EaseInOutSine,
        ],
        0.65,
        1.0,
    )
}

/// Exponential and circular rates share the same retained target composition.
pub fn exponential_circular_session() -> Result<ExecutionSession, String> {
    build_session(
        &[
            RateFunction::EaseInExpo,
            RateFunction::EaseOutExpo,
            RateFunction::EaseInOutExpo,
            RateFunction::EaseInCirc,
            RateFunction::EaseOutCirc,
            RateFunction::EaseInOutCirc,
        ],
        0.45,
        0.6,
    )
}

fn build_session(
    rates: &[RateFunction],
    side: f64,
    row_step: f64,
) -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut shapes = Vec::new();
        let mut targets = Vec::new();
        let colors = [
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.0, 1.0, 1.0),
            (1.0, 0.0, 1.0),
        ];
        for (index, _) in rates.iter().enumerate() {
            let rgb = colors[index % colors.len()];
            let mut shape = scene.square(side)?;
            shape.shift(-2.0, 2.0 - index as f64 * row_step)?;
            shape.set_color(rgb.0, rgb.1, rgb.2, 1.0)?;
            shape.set_fill_opacity(1.0)?;
            shape.set_stroke_width(0.0)?;
            scene.add(&shape)?;
            let mut target = shape.target_editor()?;
            target.shift(4.0, 0.0)?;
            shapes.push(shape);
            targets.push(target);
        }
        let children = shapes
            .iter()
            .zip(&targets)
            .zip(rates.iter().copied())
            .map(|((shape, target), rate)| {
                AnimationCompositionRequest::TransformTo(
                    TransformToRequest::new(
                        shape,
                        target,
                        AnimationOptions::new().run_time(1.0).rate_func(rate),
                    )
                    .method_target(),
                )
            })
            .collect();
        let request = AnimationCompositionRequest::Composition {
            kind: SemanticAnimationCompositionKind::Parallel,
            children,
            options: AnimationOptions::new().rate_func(RateFunction::Linear),
        };
        let mut session = scene.execution_session()?;
        scene
            .live(&mut session)
            .declare_and_activate_composition(&request, AnimationOptions::new())?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_seek_matches_forward(
        build: fn() -> Result<ExecutionSession, String>,
        expected_count: usize,
    ) -> ExecutionSession {
        let mut forward = build().unwrap();
        let mut sought = build().unwrap();
        let ids = forward
            .frame()
            .objects
            .iter()
            .map(|o| o.id)
            .collect::<Vec<_>>();
        assert_eq!(ids.len(), expected_count);
        for time in [0.0, 0.125, 0.25, 0.375, 0.5, 0.75, 1.0] {
            forward.advance_to(time).unwrap();
            sought.seek(time).unwrap();
            for (index, (left, right)) in forward
                .frame()
                .objects
                .iter()
                .zip(&sought.frame().objects)
                .enumerate()
            {
                assert_eq!(left.id, ids[index]);
                assert_eq!(left.id, right.id);
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
            }
        }
        assert_eq!(forward.effective_time(), 1.0);
        sought
    }

    #[test]
    fn exponential_circular_rates_keep_original_positions_and_seek_identity() {
        let mut sought = assert_seek_matches_forward(exponential_circular_session, 6);
        sought.seek(0.25).unwrap();
        for (object, original) in sought.frame().objects.iter().zip([
            0.005524271728019903_f32,
            0.8232233047033631_f32,
            0.015625_f32,
            0.031754163448145745_f32,
            0.6614378277661477_f32,
            0.0669872981077807_f32,
        ]) {
            assert!((object.transform.translation.x - (-2.0 + 4.0 * original)).abs() < 1e-6);
        }
    }

    #[test]
    fn sine_rates_keep_seek_and_forward_sampling_equal_at_original_positions() {
        let mut sought = assert_seek_matches_forward(sine_session, 3);
        sought.seek(0.25).unwrap();
        for (object, original) in sought.frame().objects.iter().zip([
            0.07612046748871326_f32,
            0.3826834323650898_f32,
            0.1464466094067262_f32,
        ]) {
            assert!((object.transform.translation.x - (-2.0 + 4.0 * original)).abs() < 1e-6);
        }
    }

    #[test]
    fn all_twelve_polynomial_easings_keep_identity_seek_and_forward_equal() {
        assert_seek_matches_forward(easing_session, 12);
    }

    #[test]
    fn all_five_rates_preserve_identity_and_seek_matches_forward_sampling() {
        let mut sought = assert_seek_matches_forward(session, 5);
        sought.seek(0.25).unwrap();
        let objects = &sought.frame().objects;
        assert!((objects[3].transform.translation.x - 0.6457513).abs() < 1e-6);
        assert_eq!(objects[4].transform.translation.x, -1.0);
    }
}
