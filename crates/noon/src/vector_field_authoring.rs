//! Static Manim-style ArrowVectorField authoring over shared Arrow families.
//!
//! Field, length, and color-scheme callbacks are evaluated once during authoring.
//! Prepared samples are then published atomically as ordinary nested semantic
//! families. No callback, runtime loop, or renderer-owned vector-field model
//! survives construction.

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
const DEFAULT_COLORS: [Color; 4] = [BLUE_E, GREEN, YELLOW, RED];

/// Error from deterministic static ArrowVectorField preparation or semantic publication.
#[derive(Debug)]
pub enum ArrowVectorFieldAuthoringError {
    Planning(StaticVectorFieldError),
    Authoring(AuthoringError),
    InvalidColorConfiguration(&'static str),
    NonFiniteColorSchemeOutput { sample_index: usize },
}

impl fmt::Display for ArrowVectorFieldAuthoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planning(error) => {
                write!(formatter, "static vector-field planning failed: {error}")
            }
            Self::Authoring(error) => {
                write!(formatter, "static vector-field authoring failed: {error}")
            }
            Self::InvalidColorConfiguration(reason) => {
                write!(formatter, "invalid static vector-field color configuration: {reason}")
            }
            Self::NonFiniteColorSchemeOutput { sample_index } => write!(
                formatter,
                "static vector-field color scheme returned a non-finite value at sample {sample_index}"
            ),
        }
    }
}

impl std::error::Error for ArrowVectorFieldAuthoringError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Planning(error) => Some(error),
            Self::Authoring(error) => Some(error),
            Self::InvalidColorConfiguration(_) | Self::NonFiniteColorSchemeOutput { .. } => None,
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
/// This type implements pinned ManimCE v0.21 default length/color behavior plus
/// preparation-time single colors, custom gradients, and custom scalar color schemes
/// for explicit 2D ranges. Dynamic StreamLines/updaters are deliberately excluded.
#[derive(Clone, Debug)]
pub struct ManimArrowVectorField {
    family: MobjectFamily,
    vectors: Vec<ManimArrow>,
}

enum ColorPolicy<'a> {
    Default,
    Single(Color),
    Gradient {
        colors: &'a [Color],
        min: f64,
        max: f64,
        scheme: Option<&'a mut dyn FnMut(VectorFieldPoint) -> f64>,
    },
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
        Self::from_plan_with_coloring(store, &plan, ColorPolicy::Default)
    }

    /// Variant of [`Self::create`] using an explicit displayed-length mapper.
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
        Self::from_plan_with_coloring(store, &plan, ColorPolicy::Default)
    }

    /// Publish every vector using one preparation-time single color.
    pub fn create_with_color<F>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        color: Color,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
    {
        let plan = plan_static_arrow_vector_field(field, ranges)?;
        Self::from_plan_with_coloring(store, &plan, ColorPolicy::Single(color))
    }

    /// Single-color variant with an explicit displayed-length mapper.
    pub fn create_with_length_and_color<F, L>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        length: L,
        color: Color,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
        L: FnMut(f64) -> f64,
    {
        let plan = plan_static_arrow_vector_field_with_length(field, ranges, length)?;
        Self::from_plan_with_coloring(store, &plan, ColorPolicy::Single(color))
    }

    /// Use the raw vector norm with an explicit Manim-style gradient and bounds.
    pub fn create_with_gradient<F>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        colors: &[Color],
        min: f64,
        max: f64,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
    {
        let plan = plan_static_arrow_vector_field(field, ranges)?;
        Self::from_plan_with_coloring(
            store,
            &plan,
            ColorPolicy::Gradient {
                colors,
                min,
                max,
                scheme: None,
            },
        )
    }

    /// Explicit-gradient variant with an explicit displayed-length mapper.
    pub fn create_with_length_and_gradient<F, L>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        length: L,
        colors: &[Color],
        min: f64,
        max: f64,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
        L: FnMut(f64) -> f64,
    {
        let plan = plan_static_arrow_vector_field_with_length(field, ranges, length)?;
        Self::from_plan_with_coloring(
            store,
            &plan,
            ColorPolicy::Gradient {
                colors,
                min,
                max,
                scheme: None,
            },
        )
    }

    /// Evaluate a custom scalar color scheme once per prepared raw vector, then let
    /// shared Rust own clipping and gradient interpolation.
    pub fn create_with_color_scheme<F, C>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        colors: &[Color],
        min: f64,
        max: f64,
        mut scheme: C,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
        C: FnMut(VectorFieldPoint) -> f64,
    {
        let plan = plan_static_arrow_vector_field(field, ranges)?;
        Self::from_plan_with_coloring(
            store,
            &plan,
            ColorPolicy::Gradient {
                colors,
                min,
                max,
                scheme: Some(&mut scheme),
            },
        )
    }

    /// Custom scalar color scheme plus an explicit displayed-length mapper.
    #[allow(clippy::too_many_arguments)]
    pub fn create_with_length_and_color_scheme<F, L, C>(
        store: Rc<RefCell<SemanticStore>>,
        field: F,
        ranges: VectorFieldRanges2D,
        length: L,
        colors: &[Color],
        min: f64,
        max: f64,
        mut scheme: C,
    ) -> Result<Self, ArrowVectorFieldAuthoringError>
    where
        F: FnMut(VectorFieldPoint) -> VectorFieldPoint,
        L: FnMut(f64) -> f64,
        C: FnMut(VectorFieldPoint) -> f64,
    {
        let plan = plan_static_arrow_vector_field_with_length(field, ranges, length)?;
        Self::from_plan_with_coloring(
            store,
            &plan,
            ColorPolicy::Gradient {
                colors,
                min,
                max,
                scheme: Some(&mut scheme),
            },
        )
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn vectors(&self) -> &[ManimArrow] {
        &self.vectors
    }

    fn from_plan_with_coloring(
        store: Rc<RefCell<SemanticStore>>,
        plan: &StaticVectorFieldPlan,
        mut coloring: ColorPolicy<'_>,
    ) -> Result<Self, ArrowVectorFieldAuthoringError> {
        if let ColorPolicy::Gradient {
            colors, min, max, ..
        } = &coloring
        {
            validate_gradient(colors, *min, *max)?;
        }

        // Build every inert Arrow request before touching the semantic store. The
        // batch seam then preflights every Arrow and admits all tip resources before
        // one transaction publishes the nested field family.
        let mut options = Vec::with_capacity(plan.samples.len());
        for (sample_index, sample) in plan.samples.iter().enumerate() {
            let mut vector =
                ManimArrowOptions::vector(sample.display_vector.x, sample.display_vector.y)?;
            vector.set_translation(sample.point.x, sample.point.y)?;
            let color = match &mut coloring {
                ColorPolicy::Default => gradient_color(
                    &DEFAULT_COLORS,
                    DEFAULT_MIN_COLOR_SCHEME_VALUE,
                    DEFAULT_MAX_COLOR_SCHEME_VALUE,
                    sample.raw_norm,
                ),
                ColorPolicy::Single(color) => *color,
                ColorPolicy::Gradient {
                    colors,
                    min,
                    max,
                    scheme,
                } => {
                    let value = if let Some(scheme) = scheme.as_deref_mut() {
                        let value = scheme(sample.raw_vector);
                        if !value.is_finite() {
                            return Err(ArrowVectorFieldAuthoringError::
                                NonFiniteColorSchemeOutput { sample_index });
                        }
                        value
                    } else {
                        sample.raw_norm
                    };
                    gradient_color(colors, *min, *max, value)
                }
            };
            vector.set_color(
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            )?;
            options.push(vector);
        }

        let (family, vectors) = create_arrow_batch_family(store, options)?;
        Ok(Self { family, vectors })
    }
}

fn validate_gradient(
    colors: &[Color],
    min: f64,
    max: f64,
) -> Result<(), ArrowVectorFieldAuthoringError> {
    if colors.is_empty() {
        return Err(ArrowVectorFieldAuthoringError::InvalidColorConfiguration(
            "gradient must contain at least one color",
        ));
    }
    if !min.is_finite() || !max.is_finite() {
        return Err(ArrowVectorFieldAuthoringError::InvalidColorConfiguration(
            "color-scheme bounds must be finite",
        ));
    }
    if max <= min {
        return Err(ArrowVectorFieldAuthoringError::InvalidColorConfiguration(
            "max color-scheme value must be greater than min color-scheme value",
        ));
    }
    Ok(())
}

/// Pinned ManimCE v0.21 `VectorField.pos_to_color`: clip the scalar into the
/// configured bounds, scale it across the color list, then linearly interpolate
/// adjacent RGB values. Manim's color mapping is RGB-only, so gradient alpha is 1.
fn gradient_color(colors: &[Color], min: f64, max: f64, value: f64) -> Color {
    debug_assert!(!colors.is_empty());
    debug_assert!(min.is_finite() && max.is_finite() && max > min);
    debug_assert!(value.is_finite());
    let value = value.clamp(min, max);
    let mut scaled = (value - min) / (max - min);
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
    use noon_core::{SemanticPaint, SemanticVec3, BLUE, PURPLE};
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
    fn single_color_applies_to_every_vector_family() {
        let scene = Scene::new();
        let field = ManimArrowVectorField::create_with_color(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x + 1.0, 0.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 2.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
            PURPLE,
        )
        .unwrap();

        for vector in field.vectors() {
            assert_color_close(stroke_color(vector), PURPLE);
        }
    }

    #[test]
    fn custom_gradient_clips_and_interpolates_raw_norm() {
        let scene = Scene::new();
        let colors = [RED, BLUE];
        let field = ManimArrowVectorField::create_with_gradient(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x, 0.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 3.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
            &colors,
            1.0,
            3.0,
        )
        .unwrap();

        assert_color_close(stroke_color(&field.vectors()[0]), RED);
        assert_color_close(stroke_color(&field.vectors()[1]), RED);
        let midpoint = Color::rgb(
            (RED.red + BLUE.red) * 0.5,
            (RED.green + BLUE.green) * 0.5,
            (RED.blue + BLUE.blue) * 0.5,
        );
        assert_color_close(stroke_color(&field.vectors()[2]), midpoint);
        assert_color_close(stroke_color(&field.vectors()[3]), BLUE);
    }

    #[test]
    fn custom_scheme_observes_raw_vector_not_display_length() {
        let scene = Scene::new();
        let colors = [RED, BLUE];
        let field = ManimArrowVectorField::create_with_length_and_color_scheme(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x, 0.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 2.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
            |_| 0.1,
            &colors,
            0.0,
            2.0,
            |raw| raw.x,
        )
        .unwrap();

        assert_color_close(stroke_color(&field.vectors()[0]), RED);
        let midpoint = Color::rgb(
            (RED.red + BLUE.red) * 0.5,
            (RED.green + BLUE.green) * 0.5,
            (RED.blue + BLUE.blue) * 0.5,
        );
        assert_color_close(stroke_color(&field.vectors()[1]), midpoint);
        assert_color_close(stroke_color(&field.vectors()[2]), BLUE);
    }

    #[test]
    fn non_finite_custom_scheme_is_atomic() {
        let scene = Scene::new();
        let before_nodes = scene.integration_store().borrow().len();
        let before_revision = scene.integration_store().borrow().scene_revision();
        let before_resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .stats();
        let colors = [RED, BLUE];

        let result = ManimArrowVectorField::create_with_color_scheme(
            Rc::clone(scene.integration_store()),
            |point| VectorFieldPoint::new(point.x + 1.0, 0.0),
            ranges(
                VectorFieldAxisRange::new(0.0, 1.0, 1.0),
                VectorFieldAxisRange::new(0.0, 0.0, 1.0),
            ),
            &colors,
            0.0,
            2.0,
            |raw| if raw.x > 1.0 { f64::NAN } else { raw.x },
        );

        assert!(matches!(
            result,
            Err(ArrowVectorFieldAuthoringError::NonFiniteColorSchemeOutput {
                sample_index: 1
            })
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
