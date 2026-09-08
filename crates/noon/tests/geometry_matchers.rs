use noon::{ManimGeometryOptions, Mobject, Scene};
use noon_core::SemanticPaint;
use std::rc::Rc;

fn shifted_rectangle(scene: &Scene, width: f64, height: f64, x: f64, y: f64) -> Mobject {
    let mut rectangle = scene.rectangle(width, height).unwrap();
    rectangle.shift(x, y).unwrap();
    rectangle
}

#[test]
fn family_surrounding_rectangle_uses_the_authoritative_bounds_union() {
    let scene = Scene::new();
    let first = shifted_rectangle(&scene, 2.0, 2.0, -2.0, 0.0);
    let second = shifted_rectangle(&scene, 4.0, 1.0, 3.0, 2.0);
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let bounds = family.layout_bounds().unwrap().unwrap();

    let surround = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::surrounding_rectangle(bounds, 0.25, 0.5, 0.0).unwrap(),
    )
    .unwrap();

    assert_eq!(surround.center().unwrap(), (1.0, 0.75));
    assert!((surround.width().unwrap() - 8.5).abs() <= 1e-6);
    assert!((surround.height().unwrap() - 4.5).abs() <= 1e-6);
}

#[test]
fn family_background_rectangle_preserves_union_style() {
    let scene = Scene::new();
    let first = shifted_rectangle(&scene, 1.0, 2.0, -1.0, -1.0);
    let second = shifted_rectangle(&scene, 3.0, 1.0, 2.0, 2.0);
    let family = scene.family(&[(&first).into(), (&second).into()]).unwrap();
    let bounds = family.layout_bounds().unwrap().unwrap();

    let background = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::background_rectangle(bounds, 0.0, 0.0, 0.0, 0.75).unwrap(),
    )
    .unwrap();

    assert_eq!(background.center().unwrap(), (1.0, 0.25));
    assert!((background.width().unwrap() - 5.0).abs() <= 1e-6);
    assert!((background.height().unwrap() - 4.5).abs() <= 1e-6);
    let style = background.state().unwrap().style;
    let Some(SemanticPaint::Solid(fill)) = style.fill else {
        panic!("background must retain a solid fill");
    };
    assert_eq!((fill.red, fill.green, fill.blue), (0.0, 0.0, 0.0));
    assert_eq!(style.fill_opacity, 0.75);
    assert_eq!(style.stroke_width, 0.0);
    assert_eq!(style.stroke_opacity, 0.0);
}

#[test]
fn one_leaf_family_matcher_matches_the_object_bounds_route() {
    let scene = Scene::new();
    let target = shifted_rectangle(&scene, 4.0, 2.0, 1.0, -2.0);
    let object_bounds = target.layout_bounds().unwrap().unwrap();
    let family_bounds = scene
        .family(&[(&target).into()])
        .unwrap()
        .layout_bounds()
        .unwrap()
        .unwrap();
    assert_eq!(family_bounds, object_bounds);

    let object_matcher = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::surrounding_rectangle(object_bounds, 0.25, 0.5, 0.1).unwrap(),
    )
    .unwrap();
    let family_matcher = Mobject::from_manim_geometry(
        Rc::clone(scene.store()),
        ManimGeometryOptions::surrounding_rectangle(family_bounds, 0.25, 0.5, 0.1).unwrap(),
    )
    .unwrap();

    assert_eq!(
        family_matcher.center().unwrap(),
        object_matcher.center().unwrap()
    );
    assert_eq!(
        family_matcher.width().unwrap(),
        object_matcher.width().unwrap()
    );
    assert_eq!(
        family_matcher.height().unwrap(),
        object_matcher.height().unwrap()
    );
    assert_eq!(
        family_matcher.state().unwrap().style,
        object_matcher.state().unwrap().style
    );
}
