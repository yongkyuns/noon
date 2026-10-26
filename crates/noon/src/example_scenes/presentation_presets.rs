//! Paired light/dark presentation using ordinary Text, shapes, and family layout.

use crate::{ColorRole, ExecutionSession, Scene, SpacingRole, StyleUpdate, TextRole, Theme};

pub fn session() -> Result<ExecutionSession, String> {
    build().map_err(|error| error.to_string())
}

fn build() -> Result<ExecutionSession, Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    for (theme, name, x) in [
        (Theme::dark(), "Dark", -3.35),
        (Theme::light(), "Light", 3.35),
    ] {
        let mut panel = scene.rectangle(6.2, 5.8)?;
        panel.set_style(StyleUpdate {
            fill_color: Some(theme.color(ColorRole::Background)),
            fill_opacity: Some(1.0),
            stroke_width: Some(0.0),
            ..Default::default()
        })?;
        panel.shift(x, 0.0)?;

        let title = scene.text(theme.text(name, TextRole::Title))?;
        let body = scene.text(theme.text("Meaning before styling", TextRole::Body))?;
        let caption =
            scene.text(theme.text("Ordinary Text, shared defaults", TextRole::Caption))?;
        let override_label = scene.text(
            theme
                .text("Explicit override", TextRole::Label)
                .with_font_size(24.0)
                .color(theme.color(ColorRole::Accent)),
        )?;

        let mut swatches = Vec::new();
        for index in 0..theme.categorical_count() {
            let mut swatch = scene.circle(0.16)?;
            swatch.set_style(StyleUpdate {
                fill_color: Some(theme.categorical(index)?),
                fill_opacity: Some(1.0),
                stroke_width: Some(0.0),
                ..Default::default()
            })?;
            swatches.push(swatch);
        }
        let swatches = scene.family(&swatches.iter().map(Into::into).collect::<Vec<_>>())?;
        swatches.arrange(1.0, 0.0, theme.spacing(SpacingRole::Normal), true)?;
        let content = scene.family(&[
            (&title).into(),
            (&body).into(),
            (&caption).into(),
            (&override_label).into(),
            (&swatches).into(),
        ])?;
        content.arrange(0.0, -1.0, theme.spacing(SpacingRole::Large), true)?;
        content.shift(x, 0.0)?;
        scene.add_many(&[(&panel).into(), (&content).into()])?;
    }
    scene.wait(1.0)?;
    Ok(scene.execution_session()?)
}
