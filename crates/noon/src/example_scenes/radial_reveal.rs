//! Target-only radial lifecycle paired with web/python/examples/radial_reveal.py.
use crate::{
    effects::{EffectUpdate, RadialEdge},
    AnimationOptions, Color, LiveProgram, RadialReveal, RateFunction, Scene, SceneScript,
    ScriptError,
};

pub fn program() -> Result<LiveProgram<SceneScript>, ScriptError> {
    let mut scene = Scene::new();
    let panel = scene.rectangle(4.0, 2.0)?;
    scene.set_fill(&panel, Color::rgb(0.2, 0.4, 0.9), 1.0)?;
    scene.disable_stroke(&panel)?;
    scene.into_script(move |scene| async move {
        let timing = AnimationOptions::new()
            .run_time(1.0)
            .rate_func(RateFunction::Linear);
        scene.play_with(RadialReveal::new(&panel), timing).await?;
        scene.wait(0.25).await?;
        scene
            .play_with(
                RadialReveal::new(&panel)
                    .out()
                    .edge(RadialEdge::Burn)
                    .parameters(EffectUpdate::default().seed(7))
                    .with_options(AnimationOptions::new().remover(false)),
                timing,
            )
            .await?;
        scene.wait(0.25).await?;
        // The existing canonical attachment preserves the authored burn treatment.
        scene.play_with(RadialReveal::new(&panel), timing).await?;
        scene.wait(0.25).await?;
        Ok(())
    })
}
