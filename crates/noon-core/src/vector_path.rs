//! Retained vector content and its explicitly lowered drawable commands.
use crate::{PathCommand, Rect, SemanticTransform2_5D, SemanticVec3, Transform2D, Vec2};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorPath {
    commands: Vec<PathCommand>,
    /// Manim-style raw point storage when the path was authored from a raw
    /// point array. Commands remain an explicitly lowered drawable prefix; this preserves
    /// incomplete trailing points that Manim retains for point-array queries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manim_points: Option<Vec<SemanticVec3>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    morph_target: Option<Box<VectorPath>>,
}

impl VectorPath {
    /// Number of authored commands retained by this path, including nested morph targets.
    pub fn retained_command_count(&self) -> usize {
        self.commands.len()
            + self
                .morph_target
                .as_deref()
                .map(Self::retained_command_count)
                .unwrap_or(0)
    }

    /// Whether this path contains at least one drawable segment.
    pub fn has_drawable_segments(&self) -> bool {
        let mut current = None;
        let mut subpath_start = None;
        for command in self.commands() {
            match *command {
                PathCommand::MoveTo { to } => {
                    current = Some(to);
                    subpath_start = Some(to);
                }
                PathCommand::LineTo { to }
                | PathCommand::QuadraticTo { to, .. }
                | PathCommand::CubicTo { to, .. } => {
                    if current.is_some() {
                        return true;
                    }
                    current = Some(to);
                }
                PathCommand::Close => {
                    if current
                        .zip(subpath_start)
                        .is_some_and(|(from, to)| (from - to).length() > f32::EPSILON)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Transform every endpoint/control point, including any morph target.
    pub fn transformed(&self, transform: Transform2D) -> Self {
        if self.manim_points().is_some() {
            return self.transformed_semantic(semantic_transform(transform));
        }
        let mut result = Self::new();
        for command in self.commands() {
            result = match *command {
                PathCommand::MoveTo { to } => result.move_to(transform.transform_point(to)),
                PathCommand::LineTo { to } => result.line_to(transform.transform_point(to)),
                PathCommand::QuadraticTo { control, to } => result.quadratic_to(
                    transform.transform_point(control),
                    transform.transform_point(to),
                ),
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => result.cubic_to(
                    transform.transform_point(control1),
                    transform.transform_point(control2),
                    transform.transform_point(to),
                ),
                PathCommand::Close => result.close(),
            };
        }
        if let Some(target) = self.morph_target() {
            result = result.with_morph_target(target.transformed(transform));
        }
        result
    }

    /// Bake one authored planar transform without quantizing raw coordinates.
    /// Drawable commands continue to use the explicit renderer representation.
    pub fn transformed_semantic(&self, transform: SemanticTransform2_5D) -> Self {
        let mut result = if let Some(points) = self.manim_points() {
            let points = points
                .iter()
                .map(|&point| transform.transform_xyz(point))
                .collect();
            let line_segments = !self.commands().iter().any(|command| {
                matches!(
                    command,
                    PathCommand::QuadraticTo { .. } | PathCommand::CubicTo { .. }
                )
            });
            let closed = matches!(self.commands().last(), Some(PathCommand::Close));
            Self::from_manim_points_drawing(points, line_segments, closed)
        } else {
            self.transformed(Transform2D {
                translation: Vec2::new(
                    transform.translation.x as f32,
                    transform.translation.y as f32,
                ),
                rotation: transform.rotation_z as f32,
                scale: Vec2::new(transform.scale.x as f32, transform.scale.y as f32),
            })
        };
        if let Some(target) = self.morph_target() {
            result.morph_target = Some(Box::new(target.transformed_semantic(transform)));
        }
        result
    }

    /// Whether every point, including morph targets, is finite.
    pub fn is_finite(&self) -> bool {
        let vec2_is_finite = |value: Vec2| value.x.is_finite() && value.y.is_finite();
        self.commands().iter().all(|command| match *command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => vec2_is_finite(to),
            PathCommand::QuadraticTo { control, to } => {
                vec2_is_finite(control) && vec2_is_finite(to)
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => vec2_is_finite(control1) && vec2_is_finite(control2) && vec2_is_finite(to),
            PathCommand::Close => true,
        }) && self.manim_points().is_none_or(|points| {
            points.iter().all(|point| {
                point.is_finite() && (point.x as f32).is_finite() && (point.y as f32).is_finite()
            })
        }) && self.morph_target().is_none_or(VectorPath::is_finite)
    }

    /// Whether every anchor and curve control remains finite after the supplied affine
    /// transform and center-offset subtraction. Curves stay inside the convex hull of
    /// their transformed controls, so this also bounds every runtime sample.
    pub fn has_finite_transformed_output(&self, transform: Transform2D, offset: Vec2) -> bool {
        let finite_output = |point: Vec2| {
            let output = transform.transform_point(point);
            output.x.is_finite()
                && output.y.is_finite()
                && (output.x - offset.x).is_finite()
                && (output.y - offset.y).is_finite()
        };
        self.commands().iter().all(|command| match *command {
            PathCommand::MoveTo { to } | PathCommand::LineTo { to } => finite_output(to),
            PathCommand::QuadraticTo { control, to } => finite_output(control) && finite_output(to),
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => finite_output(control1) && finite_output(control2) && finite_output(to),
            PathCommand::Close => true,
        }) && self.manim_points().is_none_or(|points| {
            points.iter().all(|point| {
                point.is_finite() && finite_output(Vec2::new(point.x as f32, point.y as f32))
            })
        }) && self
            .morph_target()
            .is_none_or(|target| target.has_finite_transformed_output(transform, offset))
    }

    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            manim_points: None,
            morph_target: None,
        }
    }

    pub fn move_to(mut self, to: Vec2) -> Self {
        self.manim_points = None;
        self.commands.push(PathCommand::MoveTo { to });
        self
    }

    pub fn line_to(mut self, to: Vec2) -> Self {
        self.manim_points = None;
        self.commands.push(PathCommand::LineTo { to });
        self
    }

    pub fn quadratic_to(mut self, control: Vec2, to: Vec2) -> Self {
        self.manim_points = None;
        self.commands.push(PathCommand::QuadraticTo { control, to });
        self
    }

    pub fn cubic_to(mut self, control1: Vec2, control2: Vec2, to: Vec2) -> Self {
        self.manim_points = None;
        self.commands.push(PathCommand::CubicTo {
            control1,
            control2,
            to,
        });
        self
    }

    pub fn close(mut self) -> Self {
        self.manim_points = None;
        self.commands.push(PathCommand::Close);
        self
    }

    /// First and last anchors, including an unfinished subpath's starting point.
    pub fn endpoints(&self) -> Option<(Vec2, Vec2)> {
        let mut first = None;
        let mut last = None;
        let mut subpath_start = None;
        for command in self.commands() {
            match *command {
                PathCommand::MoveTo { to } => {
                    first.get_or_insert(to);
                    subpath_start = Some(to);
                    last = Some(to);
                }
                PathCommand::LineTo { to }
                | PathCommand::QuadraticTo { to, .. }
                | PathCommand::CubicTo { to, .. } => last = Some(to),
                PathCommand::Close => last = subpath_start,
            }
        }
        first.zip(last)
    }

    /// Reopen the final closed contour for extension, retaining its closing edge.
    /// Earlier subpaths and their closure flags are unchanged.
    pub fn open_last_subpath(mut self) -> Self {
        if !matches!(self.commands.last(), Some(PathCommand::Close)) {
            return self;
        }
        let start = self
            .commands
            .iter()
            .rev()
            .find_map(|command| match command {
                PathCommand::MoveTo { to } => Some(*to),
                _ => None,
            });
        self.commands.pop();
        self.manim_points = None;
        if let (Some(start), Some((_, last))) = (start, self.endpoints()) {
            if last != start {
                self.commands.push(PathCommand::LineTo { to: start });
            }
        }
        self
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    pub fn with_morph_target(mut self, target: VectorPath) -> Self {
        self.morph_target = Some(Box::new(target));
        self
    }

    pub fn morph_target(&self) -> Option<&VectorPath> {
        self.morph_target.as_deref()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn conservative_bounds(&self) -> Option<Rect> {
        self.transformed_conservative_bounds(Transform2D::IDENTITY)
    }

    pub(crate) fn transformed_conservative_bounds(&self, transform: Transform2D) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        let mut include = |point: Vec2| {
            let point = transform.transform_point(point);
            match &mut bounds {
                Some(bounds) => bounds.include(point),
                None => bounds = Some(Rect::new(point, point)),
            }
        };

        for command in &self.commands {
            match *command {
                PathCommand::MoveTo { to } | PathCommand::LineTo { to } => include(to),
                PathCommand::QuadraticTo { control, to } => {
                    include(control);
                    include(to);
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    to,
                } => {
                    include(control1);
                    include(control2);
                    include(to);
                }
                PathCommand::Close => {}
            }
        }
        if let Some(points) = self.manim_points() {
            for point in points.iter().copied() {
                include(Vec2::new(point.x as f32, point.y as f32));
            }
        }
        bounds
    }
}

fn semantic_transform(transform: Transform2D) -> SemanticTransform2_5D {
    SemanticTransform2_5D {
        translation: SemanticVec3::from_vec2(transform.translation),
        scale: SemanticVec3::new(
            f64::from(transform.scale.x),
            f64::from(transform.scale.y),
            1.0,
        ),
        rotation_z: transform.rotation as f64,
    }
}

mod raw_points;

#[cfg(test)]
mod corner_tests;
