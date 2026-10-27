use super::*;
use crate::{lower_semantic_execution_root, SemanticExecutionIndex};
use noon_core::{Color, GradientStop};

#[test]
fn hatch_projection_is_periodic_bounded_and_rotation_independent() {
    use noon_core::{HatchPaint, HatchPaintOptions, Rect, Vec2};
    let make = |spacing, angle, phase| {
        HatchPaint::new(
            Color::WHITE,
            Color::TRANSPARENT,
            HatchPaintOptions {
                spacing,
                width: spacing * 0.25,
                angle,
                phase,
                cross: true,
            },
        )
        .unwrap()
    };
    let bounds = Rect::new(Vec2::new(-2.0, -1.0), Vec2::new(2.0, 1.0));
    for angle in [0.0, 0.3, std::f64::consts::FRAC_PI_2, 7.0] {
        let hatch = make(0.5, angle, [-0.25, 4_294_967_296.125]);
        let rows = project_hatch_mapping(&hatch).unwrap();
        assert_eq!(rows[0][2..], [0.75, 0.25]);
        assert_eq!(rows[1][2..], [0.125, 0.0]);
        validate_hatch_bounds(&hatch, bounds).unwrap();
        assert!(validate_hatch_bounds(&make(1e-8, angle, [0.0; 2]), bounds).is_err());
    }
    for spacing in [1e-60, 1e60] {
        assert!(project_hatch_mapping(&make(spacing, 0.7, [0.0; 2])).is_err());
    }
    let rounded_seam = project_hatch_mapping(&make(0.5, 0.0, [-1e-100, 0.0])).unwrap();
    assert_eq!(rounded_seam[0][2], 0.0);

    // A valid zero-width endpoint can pass through arbitrarily small positive
    // widths. The finite GPU duty may round to zero, never fail after admission.
    let absent = HatchPaint::new(
        Color::WHITE,
        Color::TRANSPARENT,
        HatchPaintOptions {
            width: 0.0,
            ..HatchPaintOptions::default()
        },
    )
    .unwrap();
    let present = HatchPaint::new(
        Color::WHITE,
        Color::TRANSPARENT,
        HatchPaintOptions::default(),
    )
    .unwrap();
    for progress in [f64::from_bits(1), 1e-100, 1e-38, 0.5, 1.0] {
        project_hatch_mapping(&absent.interpolate(&present, progress).unwrap()).unwrap();
    }
}

#[test]
fn radial_profile_rejects_unrepresentable_radii_and_moving_stop_gaps() {
    let make = |radii, position| {
        Arc::new(
            noon_core::RadialGradient::new(
                [0.5; 2],
                radii,
                [
                    GradientStop::new(0.0, Color::WHITE).unwrap(),
                    GradientStop::new(position, Color::BLACK).unwrap(),
                    GradientStop::new(1.0, Color::WHITE).unwrap(),
                ],
            )
            .unwrap(),
        )
    };
    for radius in [1e-60, 1e100] {
        assert!(validate_radial(&make([radius, 0.5], 0.5)).is_err());
    }
    let transition = noon_core::PaintTransition::new(
        Some(SemanticPaint::RadialGradient(make([0.5; 2], 0.0))),
        1.0,
        Some(SemanticPaint::RadialGradient(make([0.75; 2], 0.5))),
        1.0,
    )
    .unwrap();
    assert!(
        validate_transition(&transition).is_err(),
        "opening hard edges need arbitrarily small GPU stop intervals"
    );
    validate_radial(&make([0.75; 2], 0.5)).unwrap();
}

fn paint(start: [f64; 2], end: [f64; 2]) -> Arc<LinearGradient> {
    Arc::new(
        LinearGradient::new(
            start,
            end,
            [
                GradientStop::new(0.0, Color::rgba(1.0, 0.0, 0.0, 0.0)).unwrap(),
                GradientStop::new(1.0, Color::rgb(0.0, 0.0, 1.0)).unwrap(),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn lowering_preserves_continuous_definition_and_separate_opacity_domains() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    let gradient = paint([0.0, 0.5], [1.0, 0.5]);
    let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
    state.style.fill = Some(SemanticPaint::LinearGradient(gradient.clone()));
    state.style.fill_opacity = 0.25;
    state.style.object_opacity = 0.5;
    let object = store.insert_semantic_object(state);
    store.add_semantic_family_member(root, object).unwrap();
    let output =
        lower_semantic_execution_root(&store, root, &mut SemanticExecutionIndex::new()).unwrap();
    let object = &output.compiled().objects()[0];
    assert!(Arc::ptr_eq(
        object
            .fill_paint
            .as_ref()
            .unwrap()
            .as_linear_gradient()
            .unwrap(),
        &gradient
    ));
    assert_eq!(
        object.base_style.fill,
        Some(Color::rgba(1.0, 1.0, 1.0, 0.25))
    );
    assert_eq!(object.base_style.opacity, 0.5);
}

#[test]
fn gradient_profile_rejects_collapsed_gpu_axes_and_stop_intervals_before_identity_publication() {
    let mut store = SemanticStore::new();
    let root = store.insert_family();
    for gradient in [
        paint([0.0, 0.0], [1e-60, 0.0]),
        Arc::new(
            LinearGradient::new(
                [0.0, 0.0],
                [1.0, 0.0],
                [
                    GradientStop::new(0.5, Color::WHITE).unwrap(),
                    GradientStop::new(0.5 + f64::EPSILON, Color::BLACK).unwrap(),
                ],
            )
            .unwrap(),
        ),
    ] {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 1.0 });
        state.style.fill = Some(SemanticPaint::LinearGradient(gradient));
        let object = store.insert_semantic_object(state);
        store.add_semantic_family_member(root, object).unwrap();
        let mut index = SemanticExecutionIndex::new();
        assert!(lower_semantic_execution_root(&store, root, &mut index).is_err());
        assert!(index.is_empty());
        store.remove_semantic_family_member(root, object).unwrap();
    }
}

#[test]
fn animation_admission_checks_interior_gpu_axis_and_moving_stop_precision() {
    let from = paint([0.25, 0.5], [0.75, 0.5]);
    let to = paint([0.75, 0.5], [0.25, 0.50000001]);
    validate_gradient(&from).unwrap();
    validate_gradient(&to).unwrap();
    let transition = noon_core::PaintTransition::new(
        Some(SemanticPaint::LinearGradient(from)),
        1.0,
        Some(SemanticPaint::LinearGradient(to)),
        1.0,
    )
    .unwrap();
    assert!(
        validate_transition(&transition).is_err(),
        "interior axis collapses after f32 rounding"
    );
    let make = |position| {
        Arc::new(
            LinearGradient::new(
                [0.0, 0.0],
                [1.0, 0.0],
                [
                    GradientStop::new(0.0, Color::WHITE).unwrap(),
                    GradientStop::new(position, Color::BLACK).unwrap(),
                    GradientStop::new(1.0, Color::WHITE).unwrap(),
                ],
            )
            .unwrap(),
        )
    };
    let transition = noon_core::PaintTransition::new(
        Some(SemanticPaint::LinearGradient(make(0.0))),
        1.0,
        Some(SemanticPaint::LinearGradient(make(0.5))),
        1.0,
    )
    .unwrap();
    assert!(
        validate_transition(&transition).is_err(),
        "an opening hard edge has arbitrarily small GPU intervals"
    );
}

#[test]
fn image_projection_bounds_rotated_coordinates_before_narrowing() {
    use noon_core::{ImageAddressMode, ImageFit, ImageMapping, ImageUvTransform};
    let mapping = |translation, scale, angle| {
        ImageMapping::new(
            ImageFit::Stretch,
            [0.5; 2],
            ImageAddressMode::Repeat,
            ImageUvTransform::new(translation, scale, angle).unwrap(),
        )
        .unwrap()
    };
    for angle in [0.0, 0.7, 1.57, 7.0] {
        assert!(project_image_mapping(
            mapping([0.2, -0.3], [-0.7, 1.2], angle),
            [4.0, 2.0],
            [3, 2]
        )
        .is_ok());
        // Both axis coefficients can fit individually near a diagonal while a
        // later rotation would overflow. Reject that entire authored profile.
        assert!(
            project_image_mapping(mapping([0.0; 2], [1e-38, 1.0], angle), [4.0, 2.0], [3, 2])
                .is_err()
        );
        assert!(project_image_mapping(
            mapping([1e30, 0.0], [1e-12, 1.0], angle),
            [4.0, 2.0],
            [3, 2]
        )
        .is_err());
    }
    assert!(
        project_image_mapping(mapping([0.0; 2], [1e-36, 1.0], 0.0), [4.0, 2.0], [4096, 1]).is_err(),
        "finite UV coefficients must not overflow the filtered texel coordinates"
    );
}
