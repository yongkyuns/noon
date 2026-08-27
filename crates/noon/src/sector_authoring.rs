//! Shared Rust authoring semantics for Manim-compatible annular geometry.
//!
//! `AnnularSector`, `Sector`, and `Annulus` reuse the retained cubic-arc semantics
//! from [`crate::Arc`] and compose them into closed [`VectorPath`] contours. No
//! renderer primitive or frontend-owned geometry is introduced.

use crate::legacy::{IntoSnapshot, Path};
use crate::{Arc, ArcAuthoringError};
use noon_core::{Color, GeometryRef, ObjectSnapshot, PathCommand, Vec2, VectorPath, TAU, WHITE};

macro_rules! define_sector_shape {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(ObjectSnapshot);

        impl $name {
            pub fn color(mut self, color: Color) -> Self {
                self.0 = self.0.set_color(color);
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0 = self.0.shift(offset);
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0 = self.0.move_to(point);
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0 = self.0.scale_by(factor);
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0 = self.0.scale_xy(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0 = self.0.rotate_by(angle);
                self
            }

            pub fn set_fill(mut self, color: Option<Color>, opacity: Option<f32>) -> Self {
                self.0 = self.0.set_fill(color, opacity);
                self
            }

            pub fn set_stroke(mut self, color: Option<Color>, width: Option<f32>) -> Self {
                self.0 = self.0.set_stroke(color, width);
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0 = self.0.set_opacity(opacity);
                self
            }

            pub fn snapshot(&self) -> &ObjectSnapshot {
                &self.0
            }
        }

        impl IntoSnapshot for $name {
            fn into_snapshot(self) -> ObjectSnapshot {
                self.0
            }
        }
    };
}

define_sector_shape!(AnnularSector);
define_sector_shape!(Sector);
define_sector_shape!(Annulus);

fn arc_path(
    radius: f32,
    start_angle: f32,
    angle: f32,
    num_components: usize,
    center: Vec2,
) -> Result<VectorPath, ArcAuthoringError> {
    let arc = Arc::with_options(radius, start_angle, angle, num_components, center)?;
    match &arc.snapshot().geometry {
        GeometryRef::VectorPath(path) => Ok(path.clone()),
        other => unreachable!("Arc must lower to VectorPath geometry, got {other:?}"),
    }
}

fn append_command(path: VectorPath, command: PathCommand) -> VectorPath {
    match command {
        PathCommand::MoveTo { to } => path.move_to(to),
        PathCommand::LineTo { to } => path.line_to(to),
        PathCommand::QuadraticTo { control, to } => path.quadratic_to(control, to),
        PathCommand::CubicTo {
            control1,
            control2,
            to,
        } => path.cubic_to(control1, control2, to),
        PathCommand::Close => path.close(),
    }
}

fn append_path(
    mut destination: VectorPath,
    source: &VectorPath,
    skip_first_move: bool,
) -> VectorPath {
    for (index, command) in source.commands().iter().copied().enumerate() {
        if skip_first_move && index == 0 && matches!(command, PathCommand::MoveTo { .. }) {
            continue;
        }
        destination = append_command(destination, command);
    }
    destination
}

fn first_point(path: &VectorPath) -> Vec2 {
    match path.commands().first().copied() {
        Some(PathCommand::MoveTo { to }) => to,
        other => unreachable!("arc path must start with MoveTo, got {other:?}"),
    }
}

fn filled_snapshot(
    path: VectorPath,
    color: Color,
    fill_opacity: f32,
    stroke_width: f32,
) -> ObjectSnapshot {
    Path::new(path)
        .set_fill(Some(color), Some(fill_opacity))
        .set_stroke(Some(color), Some(stroke_width))
        .into_snapshot()
}

impl AnnularSector {
    /// Build ManimCE's default annular sector: inner radius 1, outer radius 2,
    /// quarter-turn angle, opaque white fill, and zero-width stroke.
    pub fn new() -> Self {
        Self::with_options(1.0, 2.0, TAU / 4.0, 0.0, 1.0, 0.0, WHITE, 9, Vec2::ZERO)
            .expect("the built-in AnnularSector default is valid")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        inner_radius: f32,
        outer_radius: f32,
        angle: f32,
        start_angle: f32,
        fill_opacity: f32,
        stroke_width: f32,
        color: Color,
        num_components: usize,
        arc_center: Vec2,
    ) -> Result<Self, ArcAuthoringError> {
        let inner = arc_path(inner_radius, start_angle, angle, num_components, arc_center)?;
        // Manim reverses an independently constructed outer arc before joining it
        // to the inner arc. Building it directly with the opposite signed angle is
        // equivalent and preserves the cubic control-point geometry.
        let outer = arc_path(
            outer_radius,
            start_angle + angle,
            -angle,
            num_components,
            arc_center,
        )?;

        let inner_start = first_point(&inner);
        let outer_end = first_point(&outer);
        let mut path = append_path(VectorPath::new(), &inner, false);
        path = path.line_to(outer_end);
        path = append_path(path, &outer, true);
        path = path.line_to(inner_start).close();

        Ok(Self(filled_snapshot(
            path,
            color,
            fill_opacity,
            stroke_width,
        )))
    }
}

impl Default for AnnularSector {
    fn default() -> Self {
        Self::new()
    }
}

impl Sector {
    pub fn new(radius: f32) -> Result<Self, ArcAuthoringError> {
        Self::with_options(radius, TAU / 4.0, 0.0, 1.0, 0.0, WHITE, 9, Vec2::ZERO)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        radius: f32,
        angle: f32,
        start_angle: f32,
        fill_opacity: f32,
        stroke_width: f32,
        color: Color,
        num_components: usize,
        arc_center: Vec2,
    ) -> Result<Self, ArcAuthoringError> {
        let sector = AnnularSector::with_options(
            0.0,
            radius,
            angle,
            start_angle,
            fill_opacity,
            stroke_width,
            color,
            num_components,
            arc_center,
        )?;
        Ok(Self(sector.into_snapshot()))
    }
}

impl Default for Sector {
    fn default() -> Self {
        Self::new(1.0).expect("the built-in Sector default is valid")
    }
}

impl Annulus {
    pub fn new(inner_radius: f32, outer_radius: f32) -> Result<Self, ArcAuthoringError> {
        Self::with_options(inner_radius, outer_radius, 1.0, 0.0, WHITE, 9, Vec2::ZERO)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_options(
        inner_radius: f32,
        outer_radius: f32,
        fill_opacity: f32,
        stroke_width: f32,
        color: Color,
        num_components: usize,
        arc_center: Vec2,
    ) -> Result<Self, ArcAuthoringError> {
        let outer = arc_path(outer_radius, 0.0, TAU, num_components, arc_center)?;
        let inner = arc_path(inner_radius, TAU, -TAU, num_components, arc_center)?;

        // Separate, oppositely wound closed contours give the retained path the
        // same annular fill semantics as Manim's outer-circle + reversed-inner-circle.
        let mut path = append_path(VectorPath::new(), &outer, false).close();
        path = append_path(path, &inner, false).close();
        Ok(Self(filled_snapshot(
            path,
            color,
            fill_opacity,
            stroke_width,
        )))
    }
}

impl Default for Annulus {
    fn default() -> Self {
        Self::new(1.0, 2.0).expect("the built-in Annulus default is valid")
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{PathCommand, StrokeWidthMode};

    use super::*;

    fn commands(snapshot: &ObjectSnapshot) -> &[PathCommand] {
        match &snapshot.geometry {
            GeometryRef::VectorPath(path) => path.commands(),
            other => panic!("expected VectorPath geometry, got {other:?}"),
        }
    }

    fn point(command: PathCommand) -> Vec2 {
        match command {
            PathCommand::MoveTo { to }
            | PathCommand::LineTo { to }
            | PathCommand::QuadraticTo { to, .. }
            | PathCommand::CubicTo { to, .. } => to,
            PathCommand::Close => panic!("Close has no explicit point"),
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn default_annular_sector_composes_inner_and_reversed_outer_arcs() {
        let sector = AnnularSector::default();
        let commands = commands(sector.snapshot());
        assert_eq!(commands.len(), 20);
        assert_eq!(point(commands[0]), Vec2::new(1.0, 0.0));
        assert_eq!(commands.last(), Some(&PathCommand::Close));
        assert_eq!(sector.snapshot().style.fill, Some(WHITE));
        assert_eq!(sector.snapshot().style.stroke, Some(WHITE));
        assert_close(sector.snapshot().style.stroke_width, 0.0);
        assert_eq!(
            sector.snapshot().style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
    }

    #[test]
    fn sector_is_annular_sector_with_degenerate_inner_radius() {
        let sector = Sector::default();
        let commands = commands(sector.snapshot());
        assert_eq!(commands.len(), 20);
        for command in &commands[..9] {
            assert_eq!(point(*command), Vec2::ZERO);
        }
        assert_eq!(commands.last(), Some(&PathCommand::Close));
    }

    #[test]
    fn annulus_has_two_closed_oppositely_wound_contours() {
        let annulus = Annulus::default();
        let commands = commands(annulus.snapshot());
        assert_eq!(commands.len(), 20);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, PathCommand::MoveTo { .. }))
                .count(),
            2
        );
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, PathCommand::Close))
                .count(),
            2
        );
        assert_eq!(point(commands[0]), Vec2::new(2.0, 0.0));
        let inner_start = point(commands[10]);
        assert_close(inner_start.x, 1.0);
        assert_close(inner_start.y, 0.0);
    }

    #[test]
    fn negative_sector_angle_reverses_outer_sweep_without_changing_start() {
        let sector =
            AnnularSector::with_options(1.0, 2.0, -TAU / 4.0, 0.0, 1.0, 0.0, WHITE, 9, Vec2::ZERO)
                .expect("negative sector angle is valid");
        let commands = commands(sector.snapshot());
        assert_eq!(point(commands[0]), Vec2::new(1.0, 0.0));
        let inner_end = point(commands[8]);
        assert_close(inner_end.x, 0.0);
        assert_close(inner_end.y, -1.0);
    }
}
