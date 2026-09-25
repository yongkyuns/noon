//! Ordinary retained BraceText families; native and browser hosts share this scene.
use crate::{BraceOptions, BraceText, LayoutAnchor, Scene, Text};

pub fn session() -> Result<crate::ExecutionSession, String> {
    scene().and_then(|scene| scene.execution_session().map_err(|error| error.to_string()))
}

fn scene() -> Result<Scene, String> {
    let build = || -> Result<Scene, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut left = scene.square(2.0)?;
        left.shift(-2.0, 0.0)?;
        let left_label = BraceText::new(
            &mut scene,
            &LayoutAnchor::from(&left),
            Text::new("Label").with_font_size(36.0),
            BraceOptions::default(),
        )?;
        let mut right = scene.rectangle(2.5, 1.5)?;
        right.shift(2.0, 0.0)?;
        let right_label = BraceText::new(
            &mut scene,
            &LayoutAnchor::from(&right),
            Text::new("Side").with_font_size(36.0),
            BraceOptions {
                direction: (1.0, 0.0),
                buff: 0.25,
                ..Default::default()
            },
        )?;
        scene.add_many(&[
            (&left).into(),
            left_label.inner().family().into(),
            (&right).into(),
            right_label.inner().family().into(),
        ])?;
        scene.wait(0.2)?;
        Ok(scene)
    };
    build().map_err(|error| error.to_string())
}
