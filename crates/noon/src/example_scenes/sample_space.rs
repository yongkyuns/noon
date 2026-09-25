//! Horizontal and vertical probability partitions paired with sample_space.py.

use crate::{
    Color, ExecutionSession, SampleSpace, SampleSpaceOptions, Scene, BLUE_E, GREEN_E, YELLOW,
};

pub fn scene() -> Result<Scene, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let options = SampleSpaceOptions {
        width: 2.8,
        height: 1.8,
        ..Default::default()
    };
    let mut horizontal = SampleSpace::new_with_options(&mut scene, &options)?;
    horizontal.divide_horizontally(&mut scene, [0.25, 0.5], &[GREEN_E, BLUE_E])?;
    horizontal.family().shift(-2.0, 0.0)?;

    let mut vertical = SampleSpace::new_with_options(&mut scene, &options)?;
    vertical.divide_vertically(
        &mut scene,
        [0.4, 0.35],
        &[Color::from_hex(0xEC92AB), YELLOW],
    )?;
    vertical.family().shift(2.0, 0.0)?;

    scene.add_many(&[horizontal.family().into(), vertical.family().into()])?;
    Ok(scene)
}

pub fn session() -> Result<ExecutionSession, String> {
    scene()
        .and_then(|scene| scene.execution_session().map_err(Into::into))
        .map_err(|error: Box<dyn std::error::Error>| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_demo_contains_two_base_rectangles_and_six_partition_rectangles() {
        let scene = scene().unwrap();
        let mut session = scene.execution_session().unwrap();
        assert_eq!(session.frame().objects.len(), 8);
        assert!(!session.has_required_callbacks());
        let revision = scene.revision();
        session.take_renderer_publication();
        session.seek(0.0).unwrap();
        assert_eq!(scene.revision(), revision);
    }
}
