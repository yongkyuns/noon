from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}")
    file.write_text(text.replace(old, new, 1))


replace_once(
    "crates/noon/src/family_affine.rs",
    '''pub(crate) enum FamilyAffine {
    Scale(f64, f64, ManimRotationPivot),
    Rotate(f64, ManimRotationPivot),
''',
    '''pub(crate) enum FamilyAffine {
    /// Existing native family scale: center-relative placement with local retained scale axes.
    Scale(f64, f64),
    /// Manim/world-pivot scale: exact only while representable without shear.
    ManimScale(f64, f64, ManimRotationPivot),
    Rotate(f64, ManimRotationPivot),
''',
)
replace_once(
    "crates/noon/src/family_affine.rs",
    '''            Self::Scale(x, y, pivot) => {
                authoring_render_f64("family scale.x", x)?;
                authoring_render_f64("family scale.y", y)?;
                resolve_pivot(bounds, center, pivot)?
            }
''',
    '''            Self::Scale(x, y) => {
                authoring_render_f64("family scale.x", x)?;
                authoring_render_f64("family scale.y", y)?;
                center
            }
            Self::ManimScale(x, y, pivot) => {
                authoring_render_f64("family scale.x", x)?;
                authoring_render_f64("family scale.y", y)?;
                resolve_pivot(bounds, center, pivot)?
            }
''',
)
replace_once(
    "crates/noon/src/family_affine.rs",
    '''                Self::Scale(x, y, _) => {
                    crate::dimension_fit::validate_fit_stretch(
                        previous.transform.rotation_z,
                        x != y,
                    )?;
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        pivot.0 + (old_center.0 - pivot.0) * x,
                        pivot.1 + (old_center.1 - pivot.1) * y,
                    );
                    scale_state_about_center(store, &mut next, x, y, target_center)?;
                }
''',
    '''                Self::Scale(x, y) => {
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        center.0 + (old_center.0 - center.0) * x,
                        center.1 + (old_center.1 - center.1) * y,
                    );
                    scale_state_about_center(store, &mut next, x, y, target_center)?;
                }
                Self::ManimScale(x, y, _) => {
                    crate::dimension_fit::validate_fit_stretch(
                        previous.transform.rotation_z,
                        x != y,
                    )?;
                    let old_center = state_center(store, previous)?;
                    let target_center = (
                        pivot.0 + (old_center.0 - pivot.0) * x,
                        pivot.1 + (old_center.1 - pivot.1) * y,
                    );
                    scale_state_about_center(store, &mut next, x, y, target_center)?;
                }
''',
)
replace_once(
    "crates/noon/src/family_affine.rs",
    '''        self.apply_affine(FamilyAffine::Scale(x, y, ManimRotationPivot::Center))
''',
    '''        self.apply_affine(FamilyAffine::Scale(x, y))
''',
)
replace_once(
    "crates/noon/src/family_affine.rs",
    '''        self.apply_affine(FamilyAffine::Scale(x, y, pivot))
''',
    '''        self.apply_affine(FamilyAffine::ManimScale(x, y, pivot))
''',
)

replace_once(
    "crates/noon/src/live_session/family_layout.rs",
    '''        let transaction = crate::family_affine::FamilyAffine::Scale(x, y, pivot)
''',
    '''        let transaction = crate::family_affine::FamilyAffine::ManimScale(x, y, pivot)
''',
)
replace_once(
    "crates/noon/src/live_session/family_layout.rs",
    '''        self.affine_family(
            family,
            crate::family_affine::FamilyAffine::Scale(x, y, crate::ManimRotationPivot::Center),
        )
''',
    '''        self.affine_family(family, crate::family_affine::FamilyAffine::Scale(x, y))
''',
)
replace_once(
    "crates/noon/src/live_session/family_layout.rs",
    '''            crate::family_affine::FamilyAffine::Scale(x, y, pivot),
''',
    '''            crate::family_affine::FamilyAffine::ManimScale(x, y, pivot),
''',
)

replace_once(
    "crates/noon-web/src/manim_scale_bridge.rs",
    '''    #[test]
    fn manim_scale_preserves_center_for_non_uniform_scale() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut line =
            Mobject::manim_line(std::rc::Rc::clone(&authoring_store), -2.0, 1.0, 3.0, 5.0).unwrap();
        line.shift(-0.5, 0.25).unwrap();
        line.rotate(-0.2).unwrap();
        let center = line.center().unwrap();

        line.manim_scale(2.0, 0.5).unwrap();

        let scaled_center = line.center().unwrap();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);
    }
''',
    '''    #[test]
    fn manim_non_uniform_scale_preserves_unrotated_center_and_rejects_world_shear_atomically() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut line =
            Mobject::manim_line(std::rc::Rc::clone(&authoring_store), -2.0, 1.0, 3.0, 5.0).unwrap();
        line.shift(-0.5, 0.25).unwrap();
        let center = line.center().unwrap();

        line.manim_scale(2.0, 0.5).unwrap();
        let scaled_center = line.center().unwrap();
        assert_close(scaled_center.0, center.0);
        assert_close(scaled_center.1, center.1);

        line.rotate(-0.2).unwrap();
        let before = line.state().unwrap();
        let revision = authoring_store.borrow().scene_revision();
        let error = line.manim_scale(2.0, 0.5).unwrap_err();
        assert!(matches!(
            error,
            noon::AuthoringError::Unsupported(
                noon::UnsupportedAuthoringOperation::RotatedDimensionStretch
            )
        ));
        assert_eq!(line.state().unwrap(), before);
        assert_eq!(authoring_store.borrow().scene_revision(), revision);
    }
''',
)

replace_once(
    "crates/noon/tests/manim_scale_pivots.rs",
    '''#[test]
fn aliased_family_scale_keeps_its_edge_and_rejects_world_shear_atomically() {
    use noon::{LayoutAnchor, ManimRotationPivot};
    let scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    let mut b = scene.square(1.).unwrap();
    a.shift(-1., 0.).unwrap();
    b.shift(1., 0.).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    LayoutAnchor::from(&family)
        .scale(2., 2., ManimRotationPivot::Edge(1., 0.))
        .unwrap();
    assert_eq!(a.center().unwrap(), (-3.5, 0.));
    assert_eq!(b.center().unwrap(), (0.5, 0.));
    b.rotate(0.3).unwrap();
    let before = [a.state().unwrap(), b.state().unwrap()];
    assert!(family.scale(2., 1.).is_err());
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
}
''',
    '''#[test]
fn native_family_scale_keeps_local_contract_while_pivoted_world_shear_rejects_atomically() {
    use noon::{LayoutAnchor, ManimRotationPivot, UnsupportedAuthoringOperation};
    let scene = Scene::new();
    let mut a = scene.square(1.).unwrap();
    let mut b = scene.square(1.).unwrap();
    a.shift(-1., 0.).unwrap();
    b.shift(1., 0.).unwrap();
    let nested = scene.family(&[(&a).into(), (&b).into()]).unwrap();
    let family = scene.family(&[(&a).into(), (&nested).into()]).unwrap();
    LayoutAnchor::from(&family)
        .scale(2., 2., ManimRotationPivot::Edge(1., 0.))
        .unwrap();
    assert_eq!(a.center().unwrap(), (-3.5, 0.));
    assert_eq!(b.center().unwrap(), (0.5, 0.));

    b.rotate(0.3).unwrap();
    let native_before = [a.state().unwrap(), b.state().unwrap()];
    family.scale(2., 1.).unwrap();
    assert_ne!([a.state().unwrap(), b.state().unwrap()], native_before);

    let before = [a.state().unwrap(), b.state().unwrap()];
    let revision = scene.revision();
    let error = LayoutAnchor::from(&family)
        .scale(2., 1., ManimRotationPivot::Center)
        .unwrap_err();
    assert!(matches!(
        error,
        noon::AuthoringError::Unsupported(UnsupportedAuthoringOperation::RotatedDimensionStretch)
    ));
    assert_eq!([a.state().unwrap(), b.state().unwrap()], before);
    assert_eq!(scene.revision(), revision);
}
''',
)
