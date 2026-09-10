//! Retained UTF-8 source selections shared by native Rust, WASM and Python.
use crate::{ExecutionSession, Scene, Text, TextSourceSpan};

pub fn session() -> Result<ExecutionSession, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut label = scene.text(Text::new("Noon é Noon").with_font_size(36.))?;
        let parts = label.text_source_parts_for("Noon")?;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].source_span, TextSourceSpan::new(0, 4));
        assert_eq!(parts[1].source_span, TextSourceSpan::new(8, 12));
        let resource = label.state()?.content.text();
        label.shift(0., 1.)?;
        label.set_color(88. / 255., 196. / 255., 221. / 255., 1.)?;
        assert_eq!(label.text_source_parts_for("Noon")?, parts);
        assert_eq!(label.state()?.content.text(), resource);
        let mut count =
            scene.text(Text::new(format!("{} source matches", parts.len())).with_font_size(28.))?;
        count.shift(0., -1.)?;
        scene.add_many(&[(&label).into(), (&count).into()])?;
        let mut session = scene.execution_session()?;
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.2)?;
        live.advance_segment_to(wait, wait.end_time())?;
        live.complete_segment(wait)?;
        Ok(session)
    };
    build().map_err(|error| error.to_string())
}
