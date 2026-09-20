//! Static implicit curves through the ordinary retained geometry constructor.
//!
//! Contouring is fully prepared before semantic identity is allocated. The
//! callback is not retained and never participates in seek or runtime work.

use crate::{ManimGeometryOptions, Mobject, PlotAuthoringError, PlotPreparationError, Scene};
use noon_core::{PathCommand, Vec2, VectorPath};
use noon_geometry::{
    change_path_anchor_mode_with_boundary, plan_isoline, validate_isoline_request, AxesFrame,
    IsolineBounds, IsolineOptions, IsolinePathError, SplineBoundary,
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

    // Map the planner's f64 coordinate points before the retained VectorPath
    // narrowing. This preserves small spans at large coordinate offsets.
    let mut path = VectorPath::new();
    for curve in &plan.curves {
        let Some(first) = curve.first().copied() else { continue };
        path = path.move_to(map_isoline_point(frame, first)?);
        let closed = curve.len() > 2 && curve.first() == curve.last();
        let end = if closed { curve.len() - 1 } else { curve.len() };
        for point in &curve[1..end] {
            path = path.line_to(map_isoline_point(frame, *point)?);
        }
        if closed {
            path = path.close();
        }
    }
    if options.use_smoothing {
        change_path_anchor_mode_with_boundary(&path, true, SplineBoundary::ManimSignedClosure)
            .map_err(|_| PlotAuthoringError::from(PlotPreparationError::SmoothingFailed).into())
    } else {
        Ok(path)
    }
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

    #[test]
    fn axes_mapping_preserves_cubic_controls_in_a_sheared_frame() {
        let frame = AxesFrame::new(
            noon_geometry::NumberLineFrame::new([0.0, 2.0, 1.0], [10.0, 20.0], [14.0, 22.0])
                .unwrap(),
            noon_geometry::NumberLineFrame::new([0.0, 4.0, 1.0], [10.0, 20.0], [6.0, 28.0])
                .unwrap(),
        );
        let path = VectorPath::new()
            .move_to(Vec2::new(0.0, 0.0))
            .cubic_to(
                Vec2::new(1.0, 2.0),
                Vec2::new(2.0, 4.0),
                Vec2::new(1.0, 4.0),
            )
            .close();
        let mapped = map_coordinate_path(path, frame).unwrap();
        assert!(matches!(
            mapped.commands(),
            [
                PathCommand::MoveTo { to },
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to: end,
                },
                PathCommand::Close,
            ] if *to == Vec2::new(10.0, 20.0)
                && *control1 == Vec2::new(10.0, 25.0)
                && *control2 == Vec2::new(10.0, 30.0)
                && *end == Vec2::new(8.0, 29.0)
        ));
    }
}
