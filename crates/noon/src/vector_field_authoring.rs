//! Static Manim-style ArrowVectorField authoring over shared Arrow families.
//!
//! Field callbacks are evaluated once during authoring by `noon-geometry`'s
//! deterministic planner. The prepared samples are then published atomically as
//! ordinary nested semantic families. No callback, runtime loop, or renderer-owned
//! vector-field model survives construction.

use crate::arrow_authoring::create_arrow_batch_family;
use crate::{AuthoringError, ManimArrow, ManimArrowOptions, MobjectFamily};
use noon_core::{Color, SemanticStore, BLUE_E, GREEN, RED, YELLOW};
use noon_geometry::{
    plan_static_arrow_vector_field, plan_static_arrow_vector_field_with_length,
    StaticVectorFieldError, StaticVectorFieldPlan, VectorFieldPoint, VectorFieldRanges2D,
};
use std::{cell::RefCell, fmt, rc::Rc};

const DEFAULT_MIN_COLOR_SCHEME_VALUE: f64 = 0.0;
const DEFAULT_MAX_COLOR_SCHEME_VALUE: f64 = 2.0;

/// Error from deterministic static ArrowVectorField preparation or semantic publication.
#[derive(Debug)]
pub enum ArrowVectorFieldAuthoringError {
    Planning(StaticVectorFieldError),
    Authoring(AuthoringError),
}

impl fmt::Display for ArrowVectorFieldAuthoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planning(error) => write!(formatter, "static vector-field planning failed: {error}"),
            Self::Authoring(error) => write!(formatter, "static vector-field authoring failed: {error}"),
        }
    }
}

impl std::error::Error for ArrowVectorFieldAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Planning(error) => Some(error),
            Self::Authoring(error) => Some(error),
        }
    }
}

impl From<StaticVectorFieldError> for ArrowVectorFieldAuthoringError {
    fn from(value: StaticVectorFieldError) -> Self {
        Self::Planning(value)
    }
}

impl From<AuthoringError> for ArrowVectorFieldAuthoringError {
    fn from(value: AuthoringError) -> Self {
        Self::Authoring(value)
    }
}

/// One static ArrowVectorField authored as an outer family of ordinary Vector families.
///
/// This slice implements pinned ManimCE v0.21 default length and default norm-color
/// behavior for explicit 2D ranges. Dynamic StreamLines/updaters are deliberately not
/// part of this type.
#[derive(Clone, Debug)]
pub struct ManimArrowVectorField {
    family: MobjectFamily,
    vectors: Vec<ManimArrow>,
}

impl ManimArrowVectorField {
    /// Evaluate `field` once over the deterministic shared planner and publish the
    /// resulting default-styled Vector families atomically.
    pub fn create<F>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
    {
        let plan = plan_static_arrow_vector_field(field, ranges)?;
        Self::from_plan(store, &plan)
    }

    /// Variant of [`Self::create`] using an explicit displayed-length mapper.
    ///
    /// The mapper is still evaluated only during authoring and is not retained as
    /// runtime or renderer behavior.
    pub fn create_with_length<F, L>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        length: L,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
        L: FnMut(f64) -> f64,
    {
        let plan = plan_static_arrow_vector_field_with_length(field, ranges, length)?;
        Self::from_plan(store, &plan)
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn vectors(&self) -> &[ManimArrow] {
        &self.vectors
    }

    fn from_plan(
        store: Rc<RefCell<SemanticStore>>,
        plan: &StaticVectorFieldPlan,
    ) -> Result<Self, ArrowVectorFieldAuthoringError> {
        // Build every inert Arrow request before touching the semantic store. The
        // batch seam then preflights every Arrow and admits all tip resources before
        // one transaction publishes the nested field family.
        let mut options = Vec::with_capacity(plan.samples.len());
        for sample in &plan.samples {
            let mut vector = ManimArrowOptions::vector(
                sample.display_vector.x,
                sample.display_vector.y,
            )?;
            vector.set_translation(sample.point.x, sample.point.y)?;
            let color = default_vector_field_color(sample.raw_norm);
            vector.set_color(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                1.0,
            )?;
            options.push(vector);
        }

        let (family, vectors) = create_arrow_batch_family(store, options)?;
        Ok(Self { family, vectors })
    }
}

/// Pinned ManimCE v0.21 default `VectorField.pos_to_color` for the default
/// norm-based scheme and `[BLUE_E, GREEN, YELLOW, RED]` gradient.
fn default_vector_field_color(norm: f64) -> Color {
    debug_assert!(norm.is_finite() && norm >= 0.0);
    let colors = [BLUE_E, GREEN, YELLOW, RED];
    let value = norm.clamp(
        DEFAULT_MIN_COLOR_SCHEME_VALUE,
        DEFAULT_MAX_COLOR_SCHEME_VALUE,
    );
    let mut scaled = (value - DEFAULT_MIN_COLOR_SCHEME_VALUE)
        / (DEFAULT_MAX_COLOR_SCHEME_VALUE - DEFAULT_MIN_COLOR_SCHEME_VALUE);
    scaled *= (colors.len() - 1) as f64;
    let index = (scaled as usize).min(colors.len() - 1);
    let next = (index + 1).min(colors.len() - 1);
    let alpha = scaled % 1.0;
    let interpolate = |left: f32, right: f32| {
        (f64::from(left) + (f64::from(right) - f64::from(left)) * alpha) as f32
    };
    Color::rgb(
        interpolate(colors[index].red, colors[next].red),
        interpolate(colors[index].green, colors[next].green),
        interpolate(colors[index].blue, colors[next].blue),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Scene;
    use noon_core::{SemanticPaint, SemanticVec3};
    use noon_geometry::VectorFieldAxisRange;

    fn ranges(x: VectorFieldAxisRange, y: VectorFieldAxisRange) -> VectorFieldRanges2D {
        VectorFieldRanges2D::new(x, y)
    }

    fn stroke_color(vector: &ManimArrow) -> Color {
        match vector.shaft().state().unwrap().style.stroke {
            Some(SemanticPaint::Solid(color)) => color,
            other => panic!("vector shaft must retain one solid stroke, got {other:?}"),
        }
    }

    fn assert_color_close(actual: Color, expected: Color) {
        assert!((actual.red - expected.red).abs() < 1e-6);
        assert!((actual.green - expected.green).abs() < 1e-6);
        assert!((actual.blue - expected.blue).abs() < 1e-6);
        assert!((actual.alpha - expected.alpha).abs() < 1e-6);
    }

    #[test]
    fn static_field_publishes_ordered_nested_vector_families() {
        let scene = Scene::new();
        let field = ManimArrowVectorField::create(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x + 1.0, point.y),
            ranges(
                VectorFieldAxisRange::new(0.0, 1.0, 1.0),
                VectorFieldAxisRange::new(0.0, 1.0, 1.0),
            ),
        )
        .unwrap();

        assert_eq!(field.vectors().len(), 4);
        let direct_members = scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(field.family().node_id())
            .unwrap();
        assert_eq!(
            direct_members,
            field
                .vectors()
                .iter()
                .map(|vector| vector.family().node_id())
                .collect::<Vec<_>>()
        );

        let translations = field
            .vectors()
            .iter()
            .map(|vector| vector.shaft().state().unwrap().transform.translation)
            .collect::<Vec<SemanticVec3>>();
        assert_eq!(
            translations,
            vec![
                SemanticVec3::new(0.0, 0.0, 0.0),
                SemanticVec3::new(0.0, 1.0, 0.0),
                SemanticVec3::new(1.0, 0.0, 0.0),
                SemanticVec3::new(1.0, 1.0, 0.0),
            ]
        );
    }

    #[test]
    fn default_norm_colors_match_pinned_gradient_endpoints_and_midpoint() {
        let scene = Scene::new();
        let field = ManimArrowVectorField::create(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x, 0.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 2.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        assert_color_close(stroke_color(&field.vectors()[0]), BLUE_E);
        let midpoint = Color::rgb(
            (GREEN.red + YELLOW.red) * 0.5,
            (GREEN.green + YELLOW.green) * 0.5,
            (GREEN.blue + YELLOW.blue) * 0.5,
        );
        assert_color_close(stroke_color(&field.vectors()[1]), midpoint);
        assert_color_close(stroke_color(&field.vectors()[2]), RED);
    }

    #[test]
    fn planning_failure_publishes_no_semantic_identity_or_tip_resource() {
        let scene = Scene::new();
        let before_nodes = scene.integration_store().borrow().len();
        let before_revision = scene.integration_store().borrow().scene_revision();
        let before_resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();

        let result = ManimArrowVectorField::create(
            Rc::clone(scene.integration_store()),
            |point| {
                if point.x == 0.0 {
                    VectorFieldPoint::new(1.0, 0.0)
                } else {
                    VectorFieldPoint::new(f64::NAN, 0.0)
                }
            },
            ranges(
                VectorFieldAxisRange::new(0.0, 1.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        );

        assert!(matches!(
            result,
            Err(ArrowVectorFieldAuthoringError::Planning(
                StaticVectorFieldError::NonFiniteFieldOutput { sample_index: 1 }
            ))
        ));
        assert_eq!(scene.integration_store().borrow().len(), before_nodes);
        assert_eq!(
            scene.integration_store().borrow().scene_revision(),
            before_revision
        );
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .stats(),
            before_resources
        );
    }

    #[test]
    fn empty_static_field_is_one_empty_semantic_family() {
        let scene = Scene::new();
        let field = ManimArrowVectorField::create(
            Rc::clone(scene.integration_store()),
            |_| VectorFieldPoint::new(1.0, 0.0),
            ranges(
                VectorFieldAxisRange::new(1.0, 0.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
        )
        .unwrap();

        assert!(field.vectors().is_empty());
        assert!(scene
            .integration_store()
            .borrow()
            .semantic_family_members_checked(field.family().node_id())
            .unwrap()
            .is_empty());
    }
}
