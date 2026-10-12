//! Native Text substring and source-range color proof.
//!
//! The selectors intentionally cover repeated matching, UTF-8 text, whitespace/newlines,
//! and a negative-end slice without overlapping one another.

use crate::{Color, ExecutionSession, Scene, Text};

pub fn session() -> Result<ExecutionSession, String> {
    let source = "Noon  café\nNoon Ω";
    let mut scene = Scene::new();
    let label = scene
        .text(
            Text::new(source)
                .with_font("DejaVu Sans Mono")
                .with_font_size(42.0)
                .color(Color::WHITE)
                .with_text2color([
                    ("No", Color::from_hex(0xEF4444)),
                    ("[6:10]", Color::from_hex(0x3B82F6)),
                    ("[-1:]", Color::from_hex(0x22C55E)),
                ]),
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&label).into()])
        .map_err(|error| error.to_string())?;
    let mut session = scene
        .execution_session()
        .map_err(|error| error.to_string())?;
    let mut live = scene.live(&mut session);
    let wait = live.wait_segment(0.2).map_err(|error| error.to_string())?;
    live.advance_segment_to(wait, wait.end_time())
        .map_err(|error| error.to_string())?;
    live.complete_segment(wait)
        .map_err(|error| error.to_string())?;
    Ok(session)
}
