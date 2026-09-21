//! Static implicit curves through the ordinary retained geometry constructor.
//!
//! Contouring is fully prepared before semantic identity is allocated. The
//! callback is not retained and never participates in seek or runtime work.

use crate::{ManimGeometryOptions, Mobject, PlotAuthoringError, PlotPreparationError, Scene};
#[cfg(test)]
use noon_core::PathCommand;
use noon_core::{Vec2, VectorPath};
use noon_geometry::{
    change_path_anchor_mode_with_boundary, plan_isoline, smooth_curve_handles,
    validate_isoline_request, AxesFrame, IsolineBounds, IsolineOptions, IsolinePathError,
    SplineBoundary,
};

/// Bounded, static implicit-curve preparation options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImplicitPlotOptions {
    pub bounds: IsolineBounds,
    pub contour: IsolineOptions,
    pub use_smoothing: bool,
    /// Maximum retained adaptive leaves, including the planner's two-leaf
    /// breadth-first budget overshoot.
    pub max_leaves: usize,
}

impl Default for ImplicitPlotOptions {
    fn default() -> Self {
        Self {
            bounds: IsolineBounds::new(
                noon_geometry::IsolinePoint::new(-64.0 / 9.0, -4.0),
                noon_geometry::IsolinePoint::new(64.0 / 9.0, 4.0),
            ),
            contour: IsolineOptions::default(),
            use_smoothing: true,
            max_leaves: 100_000,
        }
    }
}

impl ManimGeometryOptions {
    /// Prepare a static zero contour in scene coordinates.
    ///
    /// The function receives coordinate-space `(x, y)` values only while this
    /// inert request is built. The returned geometry can be styled or transformed
    /// before it is published by [`Scene::geometry`].
    pub fn implicit_plot(
        options: &ImplicitPlotOptions,
        function: impl FnMut(f64, f64) -> f64,
    ) -> Result<Self, PlotAuthoringError> {
        Ok(Self::path(prepare_implicit_path(options, function)?)?)
    }

    /// Prepare a static zero contour in one captured Axes coordinate frame.
    ///
    /// Contouring and optional smoothing occur in coordinate space. The completed
    /// retained path is then mapped once, so a nonuniform or transformed frame
    /// does not affect callback coordinates or contour topology.
    pub fn axes_implicit_plot(
        frame: AxesFrame,
        options: &ImplicitPlotOptions,
        function: impl FnMut(f64, f64) -> f64,
    ) -> Result<Self, crate::CoordinateAuthoringError> {
        let path = prepare_axes_implicit_path(frame, options, function)?;
        Ok(Self::path(path)?)
    }
}

impl Scene {
    /// Construct static implicit geometry through this Scene's normal publication
    /// route. The callback runs only during preparation, before any scene change.
    pub fn implicit_plot(
        &mut self,
        options: &ImplicitPlotOptions,
        function: impl FnMut(f64, f64) -> f64,
    ) -> Result<Mobject, PlotAuthoringError> {
        Ok(self.geometry(ManimGeometryOptions::implicit_plot(options, function)?)?)
    }
}

pub(crate) fn prepare_axes_implicit_path(
    frame: AxesFrame,
    options: &ImplicitPlotOptions,
    mut function: impl FnMut(f64, f64) -> f64,
) -> Result<VectorPath, crate::CoordinateAuthoringError> {
    let planner_budget = validate_isoline_request(options.bounds, options.contour)
        .map_err(|_| PlotAuthoringError::from(PlotPreparationError::InvalidImplicitOptions))?;
    planner_budget
        .checked_add(2)
        .filter(|leaves| *leaves <= options.max_leaves)
        .ok_or(PlotPreparationError::ImplicitLeafLimitExceeded)
        .map_err(PlotAuthoringError::from)?;

    let plan = plan_isoline(
        |point| function(point.x, point.y),
        options.bounds,
        options.contour,
    )
    .map_err(|_| PlotAuthoringError::from(PlotPreparationError::InvalidImplicitOptions))?;

    // Keep both spline boundary selection and handle preparation in the
    // planner's f64 coordinate space. Map only completed anchors/handles, then
    // narrow once at the retained-path boundary. A translation/reflection must
    // not change Manim's signed source-space closure decision.
    let mut path = VectorPath::new();
    for curve in &plan.curves {
        let Some(first) = curve.first().copied() else {
            continue;
        };
        path = path.move_to(map_isoline_point(frame, first)?);
        let closed = curve.len() > 2 && curve.first() == curve.last();
        if options.use_smoothing {
            let anchors = curve
                .iter()
                .map(|point| [point.x, point.y])
                .collect::<Vec<_>>();
            let handles = smooth_curve_handles(&anchors, SplineBoundary::ManimSignedClosure)
                .map_err(|_| PlotAuthoringError::from(PlotPreparationError::SmoothingFailed))?;
            for ([first, second], end) in handles.into_iter().zip(&curve[1..]) {
                path = path.cubic_to(
                    map_isoline_point(frame, noon_geometry::IsolinePoint::new(first[0], first[1]))?,
                    map_isoline_point(
                        frame,
                        noon_geometry::IsolinePoint::new(second[0], second[1]),
                    )?,
                    map_isoline_point(frame, *end)?,
                );
            }
        } else {
            let end = if closed { curve.len() - 1 } else { curve.len() };
            for point in &curve[1..end] {
                path = path.line_to(map_isoline_point(frame, *point)?);
            }
        }
        if closed {
            path = path.close();
        }
    }
    Ok(path)
}

fn map_isoline_point(
    frame: AxesFrame,
    point: noon_geometry::IsolinePoint,
) -> Result<Vec2, crate::CoordinateAuthoringError> {
    let [x, y] = frame.coords_to_point(point.x, point.y)?;
    let point = Vec2::new(x as f32, y as f32);
    if point.x.is_finite() && point.y.is_finite() {
        Ok(point)
    } else {
        Err(crate::CoordinateAuthoringError::Coordinate(
            noon_geometry::CoordinateError::InvalidPoint,
        ))
    }
}

pub(crate) fn prepare_implicit_path(
    options: &ImplicitPlotOptions,
    mut function: impl FnMut(f64, f64) -> f64,
) -> Result<VectorPath, PlotAuthoringError> {
    let planner_budget = validate_isoline_request(options.bounds, options.contour)
        .map_err(|_| PlotPreparationError::InvalidImplicitOptions)?;
    planner_budget
        .checked_add(2)
        .filter(|leaves| *leaves <= options.max_leaves)
        .ok_or(PlotPreparationError::ImplicitLeafLimitExceeded)?;

    let plan = plan_isoline(
        |point| function(point.x, point.y),
        options.bounds,
        options.contour,
    )
    .map_err(|_| PlotPreparationError::InvalidImplicitOptions)?;
    let path = plan.path().map_err(implicit_path_error)?;
    let path = if options.use_smoothing {
        change_path_anchor_mode_with_boundary(&path, true, SplineBoundary::ManimSignedClosure)
            .map_err(|_| PlotPreparationError::SmoothingFailed)?
    } else {
        path
    };
    Ok(path)
}

fn implicit_path_error(error: IsolinePathError) -> PlotPreparationError {
    match error {
        IsolinePathError::InvalidPoint {
            curve_index,
            point_index,
        } => PlotPreparationError::InvalidImplicitPoint {
            curve_index,
            point_index,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn invalid_admission_rejects_before_callback() {
        let calls = Cell::new(0);
        let options = ImplicitPlotOptions {
            max_leaves: 1,
            ..ImplicitPlotOptions::default()
        };
        let result = ManimGeometryOptions::implicit_plot(&options, |_, _| {
            calls.set(calls.get() + 1);
            1.0
        });
        assert!(matches!(
            result,
            Err(PlotAuthoringError::Preparation(
                PlotPreparationError::ImplicitLeafLimitExceeded
            ))
        ));
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn empty_contour_is_an_empty_retained_path() {
        let options = ImplicitPlotOptions {
            contour: IsolineOptions {
                min_depth: 0,
                max_quads: 1,
                tolerance: None,
            },
            ..ImplicitPlotOptions::default()
        };
        let path = prepare_implicit_path(&options, |_, _| 1.0).unwrap();
        assert!(path.commands().is_empty());
    }

    #[test]
    fn contours_are_smoothed_as_closed_subpaths_in_planner_order() {
        let options = ImplicitPlotOptions {
            bounds: IsolineBounds::new(
                noon_geometry::IsolinePoint::new(-2.0, -2.0),
                noon_geometry::IsolinePoint::new(2.0, 2.0),
            ),
            contour: IsolineOptions {
                min_depth: 3,
                max_quads: 256,
                tolerance: None,
            },
            max_leaves: 1_000,
            ..ImplicitPlotOptions::default()
        };
        let path = prepare_implicit_path(&options, |x, y| x * x + y * y - 1.0).unwrap();
        assert!(path
            .commands()
            .iter()
            .any(|command| matches!(command, PathCommand::Close)));
        assert!(path
            .commands()
            .iter()
            .any(|command| matches!(command, PathCommand::CubicTo { .. })));
    }

    #[test]
    fn scene_publication_does_not_retain_or_reinvoke_callback() {
        let calls = Cell::new(0);
        let options = ImplicitPlotOptions {
            contour: IsolineOptions {
                min_depth: 1,
                max_quads: 16,
                tolerance: None,
            },
            ..ImplicitPlotOptions::default()
        };
        let mut scene = Scene::new();
        let curve = scene
            .implicit_plot(&options, |x, _| {
                calls.set(calls.get() + 1);
                x
            })
            .unwrap();
        let prepared_calls = calls.get();
        scene.add(&curve).unwrap();
        let _ = scene.execution_session().unwrap();
        assert_eq!(calls.get(), prepared_calls);
    }

    #[test]
    fn rejected_implicit_request_does_not_publish_scene_state() {
        let mut scene = Scene::new();
        let sentinel = scene.sampled_plot(&[[0.0, 0.0], [1.0, 1.0]]).unwrap();
        let revision = scene.revision();
        let resources = scene
            .integration_store()
            .borrow()
            .geometry_resources()
            .len();
        let calls = Cell::new(0);
        let options = ImplicitPlotOptions {
            max_leaves: 1,
            ..ImplicitPlotOptions::default()
        };
        assert!(scene
            .implicit_plot(&options, |_, _| {
                calls.set(calls.get() + 1);
                1.0
            })
            .is_err());
        assert_eq!(calls.get(), 0);
        assert_eq!(scene.revision(), revision);
        assert_eq!(
            scene
                .integration_store()
                .borrow()
                .geometry_resources()
                .len(),
            resources
        );
        assert!(sentinel.path_query().is_ok());
    }

    fn assert_mapped_vertical_contour(x_range: [f64; 3], y_range: [f64; 3]) {
        // Use the same clamped-origin construction as real Axes. A manually
        // positioned y axis at x=0 is not coherent with a positive-only x range.
        let centered = AxesFrame::centered(x_range, y_range, 10.0, 4.0).unwrap();
        let root_x = x_range[0] + (x_range[1] - x_range[0]) * 0.375;
        let center_y = y_range[0] + (y_range[1] - y_range[0]) * 0.5;
        let expected_x = -1.25;
        let mapped_center = centered.coords_to_point(root_x, center_y).unwrap();
        assert!((mapped_center[0] - expected_x).abs() < 1.0e-6);
        assert!(mapped_center[1].abs() < 1.0e-6);

        for transformed in [false, true] {
            let project = |point: [f64; 2]| {
                if transformed {
                    // Reflection, quarter-turn, nonuniform scale and translation.
                    [3.0 - 1.5 * point[1], -2.0 - 0.75 * point[0]]
                } else {
                    point
                }
            };
            let frame = AxesFrame::new(
                noon_geometry::NumberLineFrame::new(
                    x_range,
                    project(centered.x().start()),
                    project(centered.x().end()),
                )
                .unwrap(),
                noon_geometry::NumberLineFrame::new(
                    y_range,
                    project(centered.y().start()),
                    project(centered.y().end()),
                )
                .unwrap(),
            );
            for use_smoothing in [false, true] {
                let options = ImplicitPlotOptions {
                    bounds: IsolineBounds::new(
                        noon_geometry::IsolinePoint::new(x_range[0], y_range[0]),
                        noon_geometry::IsolinePoint::new(x_range[1], y_range[1]),
                    ),
                    contour: IsolineOptions {
                        min_depth: 3,
                        max_quads: 256,
                        tolerance: None,
                    },
                    use_smoothing,
                    max_leaves: 1_000,
                };
                let field = |x: f64, y: f64| {
                    assert!((x_range[0]..=x_range[1]).contains(&x));
                    assert!((y_range[0]..=y_range[1]).contains(&y));
                    (x - root_x) / (x_range[1] - x_range[0])
                };
                let path = prepare_axes_implicit_path(frame, &options, field).unwrap();
                let points = path
                    .commands()
                    .iter()
                    .filter_map(|command| match command {
                        PathCommand::MoveTo { to }
                        | PathCommand::LineTo { to }
                        | PathCommand::QuadraticTo { to, .. }
                        | PathCommand::CubicTo { to, .. } => Some(*to),
                        PathCommand::Close => None,
                    })
                    .collect::<Vec<_>>();
                assert!(points.len() >= 2, "nonempty vertical contour: {path:?}");
                let expected = project([expected_x, 0.0]);
                let fixed = usize::from(transformed);
                let value = |point: &Vec2| {
                    if fixed == 0 {
                        f64::from(point.x)
                    } else {
                        f64::from(point.y)
                    }
                };
                assert!(
                    points.iter().all(|p| (value(p) - expected[fixed]).abs() < 0.025),
                    "mapped contour drift: range={x_range:?} transformed={transformed} smooth={use_smoothing} points={points:?}"
                );
                let varying = |point: &Vec2| {
                    if transformed {
                        f64::from(point.x)
                    } else {
                        f64::from(point.y)
                    }
                };
                let min = points.iter().map(varying).fold(f64::INFINITY, f64::min);
                let max = points.iter().map(varying).fold(f64::NEG_INFINITY, f64::max);
                // The adaptive trace may terminate inside boundary cells. Test
                // mapping against its actual f64 anchor extent, not an invented
                // requirement that it touch the exact requested domain bounds.
                let reference =
                    plan_isoline(|p| field(p.x, p.y), options.bounds, options.contour).unwrap();
                let mapped = reference
                    .curves
                    .iter()
                    .flatten()
                    .map(|point| {
                        let scene_y =
                            4.0 * ((point.y - y_range[0]) / (y_range[1] - y_range[0]) - 0.5);
                        project([expected_x, scene_y])[1 - fixed]
                    })
                    .collect::<Vec<_>>();
                let expected_min = mapped.iter().copied().fold(f64::INFINITY, f64::min);
                let expected_max = mapped.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                assert!(expected_max - expected_min > if transformed { 5.0 } else { 3.5 });
                assert!(
                    (min - expected_min).abs() < 2.0e-5,
                    "{min} != {expected_min}"
                );
                assert!(
                    (max - expected_max).abs() < 2.0e-5,
                    "{max} != {expected_max}"
                );
            }
        }
    }

    #[test]
    fn axes_mapping_preserves_small_spans_at_large_positive_and_negative_offsets() {
        for start in [1_000_000_000.0, -1_000_000_010.0] {
            assert_mapped_vertical_contour([start, start + 10.0, 1.0], [-1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn axes_mapping_preserves_tiny_domains_and_values_outside_f32_range() {
        for extent in [1.0e-60, 1.0e45] {
            assert_mapped_vertical_contour([-extent, extent, extent], [-extent, extent, extent]);
        }
    }
    #[test]
    fn axes_smoothing_boundary_is_selected_before_frame_mapping() {
        let base = AxesFrame::centered([1.0, 3.0, 1.0], [1.0, 3.0, 1.0], 4.0, 4.0).unwrap();
        let translate = |point: [f64; 2]| [point[0] + 10.0, point[1] + 10.0];
        let translated = AxesFrame::new(
            noon_geometry::NumberLineFrame::new(
                base.x().range(),
                translate(base.x().start()),
                translate(base.x().end()),
            )
            .unwrap(),
            noon_geometry::NumberLineFrame::new(
                base.y().range(),
                translate(base.y().start()),
                translate(base.y().end()),
            )
            .unwrap(),
        );
        let options = ImplicitPlotOptions {
            bounds: IsolineBounds::new(
                noon_geometry::IsolinePoint::new(1.0, 1.0),
                noon_geometry::IsolinePoint::new(3.0, 3.0),
            ),
            contour: IsolineOptions {
                min_depth: 2,
                max_quads: 64,
                tolerance: None,
            },
            max_leaves: 100,
            use_smoothing: true,
        };
        let circle = |x: f64, y: f64| (x - 2.0).powi(2) + (y - 2.0).powi(2) - 0.36;
        let original = prepare_axes_implicit_path(base, &options, circle).unwrap();
        let shifted = prepare_axes_implicit_path(translated, &options, circle).unwrap();
        assert_eq!(original.commands().len(), shifted.commands().len());
        let near = |left: Vec2, right: Vec2| {
            assert!(
                (right.x - 10.0 - left.x).abs() < 2.0e-5,
                "{left:?} {right:?}"
            );
            assert!(
                (right.y - 10.0 - left.y).abs() < 2.0e-5,
                "{left:?} {right:?}"
            );
        };
        let mut cubics = 0;
        for (left, right) in original.commands().iter().zip(shifted.commands()) {
            match (*left, *right) {
                (PathCommand::MoveTo { to: a }, PathCommand::MoveTo { to: b }) => near(a, b),
                (
                    PathCommand::CubicTo {
                        control1: a1,
                        control2: a2,
                        to: a,
                    },
                    PathCommand::CubicTo {
                        control1: b1,
                        control2: b2,
                        to: b,
                    },
                ) => {
                    near(a1, b1);
                    near(a2, b2);
                    near(a, b);
                    cubics += 1;
                }
                (PathCommand::Close, PathCommand::Close) => {}
                _ => panic!("unexpected contour command pair: {left:?} {right:?}"),
            }
        }
        assert!(cubics > 3);
    }
}
