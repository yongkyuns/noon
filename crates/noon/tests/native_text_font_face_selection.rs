#![cfg(all(feature = "native-text", feature = "bundled-fonts"))]

use noon::{integration::RetainedScene, Text};
use swash::{FontRef, Stretch, Style, Weight};

#[test]
fn family_lookup_prefers_regular_upright_normal_stretch_face() {
    let mut scene = RetainedScene::new();
    scene
        .add_text(Text::new("Noon").with_font("DejaVu Sans Mono"))
        .unwrap();

    let handle = scene.objects()[0]
        .content
        .text()
        .expect("native Text must retain text content");
    let resource = scene
        .texts()
        .get(handle)
        .expect("native Text resource must remain retained");
    let run = resource
        .runs
        .first()
        .expect("plain native Text must have a glyph run");
    let retained_font = scene
        .fonts()
        .get_for_face(&run.font)
        .expect("glyph run must retain its exact font bytes");
    let face = FontRef::from_index(retained_font.data.as_ref(), run.font.face_index as usize)
        .expect("retained bundled font must be a valid OpenType face");
    let attributes = face.attributes();

    assert_eq!(attributes.weight(), Weight::NORMAL);
    assert_eq!(attributes.style(), Style::Normal);
    assert_eq!(attributes.stretch(), Stretch::NORMAL);
}
