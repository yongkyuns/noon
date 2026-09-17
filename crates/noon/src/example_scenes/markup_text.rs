//! Paired native/WASM MarkupText example exercising supported inline styles.
use crate::{ExecutionSession, MarkupText, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let title = scene
        .text(
            MarkupText::new("<b>Noon</b> <i>markup</i> <tt>&lt;Rust&gt;</tt>\n<span foreground=\"#58c4dd\">bold</span> and <span fgcolor=\"#ff862f\">color</span>")
                .with_font("DejaVu Sans Mono")
                .with_font_size(42.0),
        )
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[(&title).into()])
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
