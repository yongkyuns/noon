//! Shared deterministic rate tracks, paired with ordinary_polynomial_rates.py.
use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut shapes = Vec::new();
        let mut targets = Vec::new();
        for (index, rgb) in [
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.0, 1.0, 1.0),
            (1.0, 0.0, 1.0),
        ]
        .into_iter()
        .enumerate()
        {
            let mut shape = scene.square(0.65)?;
            shape.shift(-2.0, 2.0 - index as f64)?;
            shape.set_color(rgb.0, rgb.1, rgb.2, 1.0)?;
            shape.set_fill_opacity(1.0)?;
            shape.set_stroke_width(0.0)?;
            scene.add(&shape)?;
            let mut target = shape.target_editor()?;
            target.shift(4.0, 0.0)?;
            shapes.push(shape);
            targets.push(target);
        }
        let rates = [
            RateFunction::Smoothstep,
            RateFunction::Smootherstep,
            RateFunction::Smoothererstep,
            RateFunction::SlowInto,
            RateFunction::DoubleSmooth,
        ];
        let children = shapes
            .iter()
            .zip(&targets)
            .zip(rates)
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

    #[test]
    fn all_five_rates_preserve_identity_and_seek_matches_forward_sampling() {
        let mut forward = session().unwrap();
        let mut sought = session().unwrap();
        let ids: Vec<_> = forward
            .frame()
            .objects
            .iter()
            .map(|object| object.id)
            .collect();
        assert_eq!(ids.len(), 5);
        for time in [0.0, 0.25, 0.5, 0.75, 1.0] {
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
                assert_eq!(left.transform, right.transform);
                assert_eq!(left.style, right.style);
            }
        }
        sought.seek(0.25).unwrap();
        let objects = &sought.frame().objects;
        assert!((objects[3].transform.translation.x - 0.6457513).abs() < 1e-6);
        assert_eq!(objects[4].transform.translation.x, -1.0);
    }
}
