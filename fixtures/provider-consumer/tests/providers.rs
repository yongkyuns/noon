#[cfg(any(
    feature = "typst",
    all(feature = "native-text", not(feature = "bundled-fonts"))
))]
use noon::TextAuthoringError;
#[cfg(any(feature = "native-text", feature = "typst"))]
use noon::{Scene, Vec2};
#[cfg(any(feature = "native-text", feature = "typst"))]
use std::sync::Arc;

/// Test input is supplied by the host, not a Cargo dependency or bundled asset.
#[cfg(any(feature = "native-text", feature = "typst"))]
fn host_font() -> Arc<[u8]> {
    let path = std::env::var_os("NOON_PROVIDER_TEST_FONT")
        .expect("set NOON_PROVIDER_TEST_FONT to a valid TrueType/OpenType font");
    std::fs::read(path).expect("read host-supplied font").into()
}

#[cfg(any(feature = "native-text", feature = "typst"))]
fn assert_shared_resource_path(mut scene: Scene, label: noon::Mobject) {
    let circle = scene.circle(0.5).unwrap();
    scene.add(&circle).unwrap();
    scene.add(&label).unwrap();
    let resource = label.state().unwrap().content.text().unwrap();
    let stats = scene.integration_store().borrow().text_resources().stats();
    let mut session = scene.execution_session().unwrap();
    assert!(session.frame().objects[0].geometry().is_some());
    assert_eq!(session.frame().objects[1].text(), Some(resource));
    assert_ne!(circle.node_id(), label.node_id());
    let mut live = scene.live(&mut session);
    live.set_translation(&label, 2.0, 1.0).unwrap();
    assert_eq!(
        live.effective(&label).unwrap().transform.translation,
        Vec2::new(2.0, 1.0)
    );
    assert_eq!(
        live.authored(&label).unwrap().content.text(),
        Some(resource)
    );
    assert_eq!(
        scene.integration_store().borrow().text_resources().stats(),
        stats
    );
}

#[cfg(feature = "native-text")]
#[test]
fn native_text_with_explicit_font_uses_the_shared_store() {
    let scene = Scene::new();
    let face = noon::NativeFontFace::new("Host font", host_font(), 0).unwrap();
    let text = noon::Text::new("Noon").with_font_face(face);
    assert_eq!(text.font_family(), "Host font");
    let label = scene.text(text).unwrap();
    assert_shared_resource_path(scene, label);
}

#[cfg(all(feature = "native-text", not(feature = "bundled-fonts")))]
#[test]
fn missing_native_fonts_do_not_mutate_resources_or_identity() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    let stats = scene.integration_store().borrow().text_resources().stats();
    assert!(matches!(
        scene.text("No font"),
        Err(TextAuthoringError::FontUnavailable(_))
    ));
    // Selecting a family after an explicit face deliberately clears the override.
    let face = noon::NativeFontFace::new("Host font", host_font(), 0).unwrap();
    let text = noon::Text::new("No font")
        .with_font_face(face)
        .with_font("Missing");
    assert!(matches!(
        scene.text(text),
        Err(TextAuthoringError::FontUnavailable(_))
    ));
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(
        scene.integration_store().borrow().text_resources().stats(),
        stats
    );
    let mut live = scene.live(&mut session);
    assert!(live.create_text(noon::Text::new("No font")).is_err());
    live.set_translation(&circle, 3.0, 0.0).unwrap();
    assert_eq!(
        live.effective(&circle).unwrap().transform.translation.x,
        3.0
    );
    assert_eq!(
        scene.integration_store().borrow().text_resources().stats(),
        stats
    );
}

#[cfg(feature = "typst")]
#[test]
fn typst_with_explicit_fonts_uses_the_shared_store() {
    let scene = Scene::new();
    let text = noon::Typst::new("#set text(font: \"DejaVu Sans\")\nNoon").with_fonts([host_font()]);
    let label = scene.typst(text).unwrap();
    assert_shared_resource_path(scene, label);
}

#[cfg(feature = "typst")]
#[test]
fn invalid_or_empty_explicit_typst_fonts_never_fall_back() {
    let scene = Scene::new();
    let revision = scene.integration_store().borrow().scene_revision();
    let stats = scene.integration_store().borrow().text_resources().stats();
    let error = scene
        .typst(noon::Typst::new("Noon").with_fonts([]))
        .unwrap_err();
    assert_eq!(
        error,
        TextAuthoringError::Typst(noon::TypstBackendError::FontsUnavailable)
    );
    let error = scene
        .typst(noon::Typst::new("Noon").with_fonts([Arc::<[u8]>::from(&b"bad font"[..])]))
        .unwrap_err();
    assert_eq!(
        error,
        TextAuthoringError::Typst(noon::TypstBackendError::InvalidFontData { index: 0 })
    );
    let error = scene
        .math_typst(noon::MathTypst::new("x").with_fonts([]))
        .unwrap_err();
    assert_eq!(
        error,
        TextAuthoringError::Typst(noon::TypstBackendError::FontsUnavailable)
    );
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    assert_eq!(
        scene.integration_store().borrow().text_resources().stats(),
        stats
    );
}

#[cfg(all(feature = "typst", not(feature = "bundled-fonts")))]
#[test]
fn unavailable_typst_fonts_leave_the_live_session_usable() {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0).unwrap();
    scene.add(&circle).unwrap();
    let mut session = scene.execution_session().unwrap();
    let revision = scene.integration_store().borrow().scene_revision();
    assert_eq!(
        scene.typst(noon::Typst::new("Noon")).unwrap_err(),
        TextAuthoringError::Typst(noon::TypstBackendError::FontsUnavailable)
    );
    assert_eq!(
        scene.integration_store().borrow().scene_revision(),
        revision
    );
    let mut live = scene.live(&mut session);
    assert!(live.create_typst(noon::Typst::new("Noon")).is_err());
    assert!(live.create_math_typst(noon::MathTypst::new("x")).is_err());
    live.set_translation(&circle, 3.0, 0.0).unwrap();
    assert_eq!(
        live.effective(&circle).unwrap().transform.translation.x,
        3.0
    );
    assert!(scene
        .integration_store()
        .borrow()
        .text_resources()
        .is_empty());
}

#[cfg(all(feature = "native-text", feature = "bundled-fonts"))]
#[test]
fn convenience_native_fonts_still_work() {
    let scene = Scene::new();
    let label = scene.text("Noon").unwrap();
    assert_shared_resource_path(scene, label);
}

#[cfg(all(feature = "typst", feature = "bundled-fonts"))]
#[test]
fn convenience_typst_and_math_still_work() {
    let scene = Scene::new();
    let label = scene.typst(noon::Typst::new("Noon")).unwrap();
    assert_shared_resource_path(scene, label);
    let scene = Scene::new();
    let equation = scene
        .math_typst(noon::MathTypst::new("frac(x, 2)"))
        .unwrap();
    assert_shared_resource_path(scene, equation);
}
