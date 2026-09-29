//! Run `cargo run -p noon-native --example native_translation_drag`.
//! Drag the green rectangle; click the blue circle to indicate it.
//! The ordinary native host forwards input; Rust owns capture and translation.
use noon::{
    AnimationOptions, ContinuationStep, IndicateOptions, LiveContinuation, LiveSession, Scene,
    BLUE, GREEN,
};

struct Finish;
impl LiveContinuation for Finish {
    type Error = std::convert::Infallible;
    fn resume(&mut self, _: &mut LiveSession<'_>) -> Result<ContinuationStep, Self::Error> {
        Ok(ContinuationStep::Finished)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let mut circle = scene.circle(0.9)?;
    circle.set_translation(-2.0, 0.0)?;
    circle.set_fill(BLUE.red.into(), BLUE.green.into(), BLUE.blue.into(), 0.75)?;
    let mut rectangle = scene.rectangle(2.2, 1.6)?;
    rectangle.set_translation(2.0, 0.0)?;
    rectangle.set_fill(
        GREEN.red.into(),
        GREEN.green.into(),
        GREEN.blue.into(),
        0.75,
    )?;
    scene.add(&circle)?;
    scene.add(&rectangle)?;
    scene.on_click_indicate(
        &circle,
        IndicateOptions::default(),
        AnimationOptions::new().run_time(0.4),
    )?;
    let mut program = scene.into_live_program(Finish)?;
    program.set_translation_drag_targets([&rectangle])?;
    noon_native::run_live_program(program)?;
    Ok(())
}
