use std::sync::Arc;

use noon_text_native::{NativeFontFace, NativeTextCompiler, NativeTextOptions};

fn bundled_font() -> NativeFontFace {
    let bytes = typst_assets::fonts()
        .next()
        .expect("Typst test assets include at least one font");
    NativeFontFace::new("Bundled Test Font", Arc::<[u8]>::from(bytes), 0).unwrap()
}

#[test]
fn conservative_native_glyph_bounds_extend_below_the_baseline() {
    let font = bundled_font();
    let mut compiler = NativeTextCompiler::new();
    let artifact = compiler
        .compile_plain("Hg", &font, &NativeTextOptions::new(48.0))
        .unwrap();
    let run = artifact.resource.runs.first().expect("one shaped line");

    assert!(!run.glyphs.is_empty());
    for glyph in run.glyphs.iter() {
        assert!(
            glyph.bounds.min.y < 0.0,
            "descent is a positive metric distance but must map below the baseline"
        );
        assert!(glyph.bounds.max.y > 0.0);
    }
}
