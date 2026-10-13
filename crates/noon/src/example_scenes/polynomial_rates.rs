//! Shared polynomial rate tracks, paired with ordinary_polynomial_rates.py.
use crate::{
    AnimationCompositionRequest, AnimationOptions, ExecutionSession, RateFunction, Scene,
    SemanticAnimationCompositionKind, TransformToRequest,
};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut shapes = Vec::new();
        let mut targets = Vec::new();
        for (index, rgb) in [(1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)]
            .into_iter()
            .enumerate()
        {
            let mut shape = scene.square(0.65)?;
            shape.shift(-2.0, 1.0 - index as f64)?;
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
