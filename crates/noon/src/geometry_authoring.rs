//! Shared retained geometry authoring, including the canonical Manim Triangle and Brace paths.
use crate::{
    family_layout::union_bounds,
    semantic_mobject::{authoring_render_f64, layout_for_content},
    AuthoringError, LayoutAnchor, ManimGeometryOptions, Mobject, Scene,
};
use noon_core::{Bounds2D64, SemanticNodeKind, SemanticTransform2_5D, Vec2, VectorPath, TAU};
use std::rc::Rc;

const BRACE_DEFAULT_MIN_WIDTH: f64 = 0.90552;

pub(crate) fn manim_triangle_path() -> VectorPath {
    let vertices: Vec<_> = (0..3)
        .map(|index| {
            let angle = TAU / 4.0 + index as f32 * (TAU / 3.0);
            Vec2::new(angle.cos(), angle.sin())
        })
        .collect();
    VectorPath::new()
        .move_to(vertices[0])
        .line_to(vertices[1])
        .line_to(vertices[2])
        .close()
}

impl ManimGeometryOptions {
    /// Construct the ManimCE v0.21 Brace path around an authored object or family.
    ///
    /// The target is observed without mutation. Upstream's temporary rotate/measure/
    /// rotate-back sequence is evaluated mathematically against the same retained
    /// semantic content so aliases never see a transient target transform.
    pub fn brace(
        target: &LayoutAnchor,
        direction: (f64, f64),
        buff: f64,
        sharpness: f64,
    ) -> Result<Self, AuthoringError> {
        let angle = brace_angle(direction)?;
        let projected = projected_layout_bounds(target, -angle)?;
        brace_options(projected, angle, buff, sharpness)
    }

    /// Construct ManimCE v0.21 BraceBetweenPoints without allocating a temporary Line.
    pub fn brace_between_points(
        point_1: (f64, f64),
        point_2: (f64, f64),
        mut direction: (f64, f64),
        buff: f64,
        sharpness: f64,
    ) -> Result<Self, AuthoringError> {
        let point_1 = finite_point("point_1", point_1)?;
        let point_2 = finite_point("point_2", point_2)?;
        if direction == (0.0, 0.0) {
            let line_x = point_2.0 - point_1.0;
            let line_y = point_2.1 - point_1.1;
            direction = (line_y, -line_x);
        }
        let angle = brace_angle(direction)?;
        let projected_1 = rotate_point(point_1, -angle);
        let projected_2 = rotate_point(point_2, -angle);
        let projected = Bounds2D64 {
            min_x: projected_1.0.min(projected_2.0),
            min_y: projected_1.1.min(projected_2.1),
            max_x: projected_1.0.max(projected_2.0),
            max_y: projected_1.1.max(projected_2.1),
        };
        brace_options(projected, angle, buff, sharpness)
    }
}

impl Scene {
    /// Construct a detached Brace in this scene's semantic store.
    pub fn brace(
        &self,
        target: &LayoutAnchor,
        direction: (f64, f64),
        buff: f64,
        sharpness: f64,
    ) -> Result<Mobject, AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), target.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        self.geometry(ManimGeometryOptions::brace(
            target, direction, buff, sharpness,
        )?)
    }

    /// Construct a detached BraceBetweenPoints in this scene's semantic store.
    pub fn brace_between_points(
        &self,
        point_1: (f64, f64),
        point_2: (f64, f64),
        direction: (f64, f64),
        buff: f64,
        sharpness: f64,
    ) -> Result<Mobject, AuthoringError> {
        self.geometry(ManimGeometryOptions::brace_between_points(
            point_1, point_2, direction, buff, sharpness,
        )?)
    }
}

fn brace_options(
    projected_target: Bounds2D64,
    angle: f64,
    buff: f64,
    sharpness: f64,
) -> Result<ManimGeometryOptions, AuthoringError> {
    let buff = authoring_render_f64("brace buff", buff)?;
    let sharpness = authoring_render_f64("brace sharpness", sharpness)?;
    let target_width = authoring_render_f64("brace target width", projected_target.width())?;
    let linear_section_length = authoring_render_f64(
        "brace linear section length",
        ((target_width * sharpness - BRACE_DEFAULT_MIN_WIDTH) / 2.0).max(0.0),
    )?;
    let raw = RawBracePath::manim_v021(linear_section_length);
    let bounds = raw.bounds();
    let raw_width = bounds.max_x - bounds.min_x;
    debug_assert!(raw_width > 0.0);

    // VMobjectFromSVGPath keeps SVG coordinates, then Brace.flip(RIGHT) mirrors Y.
    // stretch_to_fit_width() scales X around the center and the following shift
    // aligns the brace's upper-left corner to the target's lower-left + buff*DOWN.
    // These two affine operations collapse to the expressions below.
    let scale_x = target_width / raw_width;
    let projected_top = -bounds.min_y;
    let mut path = VectorPath::new();
    for command in raw.commands {
        path = match command {
            RawPathCommand::Move(to) => path.move_to(lower_brace_point(
                to,
                bounds.min_x,
                scale_x,
                projected_target,
                projected_top,
                buff,
                angle,
            )?),
            RawPathCommand::Line(to) => path.line_to(lower_brace_point(
                to,
                bounds.min_x,
                scale_x,
                projected_target,
                projected_top,
                buff,
                angle,
            )?),
            RawPathCommand::Cubic {
                control1,
                control2,
                to,
            } => path.cubic_to(
                lower_brace_point(
                    control1,
                    bounds.min_x,
                    scale_x,
                    projected_target,
                    projected_top,
                    buff,
                    angle,
                )?,
                lower_brace_point(
                    control2,
                    bounds.min_x,
                    scale_x,
                    projected_target,
                    projected_top,
                    buff,
                    angle,
                )?,
                lower_brace_point(
                    to,
                    bounds.min_x,
                    scale_x,
                    projected_target,
                    projected_top,
                    buff,
                    angle,
                )?,
            ),
            RawPathCommand::Close => path.close(),
        };
    }

    let mut options = ManimGeometryOptions::path(path)?;
    options.set_fill_opacity(1.0)?;
    options.set_stroke_width(0.0)?;
    Ok(options)
}

fn lower_brace_point(
    raw: Point64,
    raw_min_x: f64,
    scale_x: f64,
    target: Bounds2D64,
    projected_top: f64,
    buff: f64,
    angle: f64,
) -> Result<Vec2, AuthoringError> {
    let flipped = (raw.x, -raw.y);
    let projected = (
        target.min_x + (flipped.0 - raw_min_x) * scale_x,
        target.min_y - buff + (flipped.1 - projected_top),
    );
    let world = rotate_point(projected, angle);
    Ok(Vec2::new(
        authoring_render_f64("brace point.x", world.0)? as f32,
        authoring_render_f64("brace point.y", world.1)? as f32,
    ))
}

fn projected_layout_bounds(
    target: &LayoutAnchor,
    rotation: f64,
) -> Result<Bounds2D64, AuthoringError> {
    let node = target.resolve()?;
    let store_rc = target.integration_store();
    let leaves = {
        let store = store_rc.borrow();
        if matches!(
            store.node(node).map(|node| node.kind()),
            Some(SemanticNodeKind::Family(_))
        ) {
            store
                .ordered_leaf_nodes(node)
                .map_err(AuthoringError::from)?
        } else {
            vec![node]
        }
    };

    let mut bounds = None;
    let store = store_rc.borrow();
    for leaf in leaves {
        let state = store
            .semantic_object_state_checked(leaf)
            .map_err(AuthoringError::from)?;
        let transform = rotate_transform_about_origin(state.transform, rotation)?;
        union_bounds(
            &mut bounds,
            layout_for_content(&store, state.content, transform)?,
        );
    }
    Ok(bounds.unwrap_or_else(|| Bounds2D64::point(0.0, 0.0)))
}

fn rotate_transform_about_origin(
    mut transform: SemanticTransform2_5D,
    angle: f64,
) -> Result<SemanticTransform2_5D, AuthoringError> {
    let angle = authoring_render_f64("brace target rotation", angle)?;
    let (sine, cosine) = angle.sin_cos();
    let x = transform.translation.x;
    let y = transform.translation.y;
    transform.translation.x =
        authoring_render_f64("brace projected translation.x", x * cosine - y * sine)?;
    transform.translation.y =
        authoring_render_f64("brace projected translation.y", x * sine + y * cosine)?;
    transform.rotation_z =
        authoring_render_f64("brace projected rotation", transform.rotation_z + angle)?;
    Ok(transform)
}

fn brace_angle(direction: (f64, f64)) -> Result<f64, AuthoringError> {
    let direction = finite_point("brace direction", direction)?;
    authoring_render_f64(
        "brace angle",
        -direction.0.atan2(direction.1) + std::f64::consts::PI,
    )
}

fn finite_point(name: &str, point: (f64, f64)) -> Result<(f64, f64), AuthoringError> {
    Ok((
        authoring_render_f64(&format!("{name}.x"), point.0)?,
        authoring_render_f64(&format!("{name}.y"), point.1)?,
    ))
}

fn rotate_point(point: (f64, f64), angle: f64) -> (f64, f64) {
    let (sine, cosine) = angle.sin_cos();
    (
        point.0 * cosine - point.1 * sine,
        point.0 * sine + point.1 * cosine,
    )
}

#[derive(Clone, Copy, Debug)]
struct Point64 {
    x: f64,
    y: f64,
}

impl Point64 {
    const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn offset(self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

#[derive(Clone, Copy, Debug)]
enum RawPathCommand {
    Move(Point64),
    Line(Point64),
    Cubic {
        control1: Point64,
        control2: Point64,
        to: Point64,
    },
    Close,
}

#[derive(Debug)]
struct RawBracePath {
    current: Point64,
    commands: Vec<RawPathCommand>,
}

impl RawBracePath {
    fn manim_v021(linear_section_length: f64) -> Self {
        let mut path = Self {
            current: Point64::new(0.0, 0.0),
            commands: Vec::with_capacity(28),
        };
        path.move_to(0.01216, 0.0);
        path.cubic_rel(-0.01152, 0.0, -0.01216, 0.0006103, -0.01216, 0.01311);
        path.line_rel(0.0, 0.007762);
        path.cubic_rel(0.06776, 0.122, 0.1799, 0.1455, 0.2307, 0.1455);
        path.line_rel(linear_section_length, 0.0);
        path.cubic_rel(0.03046, 0.0003899, 0.07964, 0.00449, 0.1246, 0.02636);
        path.cubic_rel(0.0537, 0.02695, 0.07418, 0.05816, 0.08648, 0.07769);
        path.cubic_rel(0.001562, 0.002538, 0.004539, 0.002563, 0.01098, 0.002563);
        path.cubic_rel(0.006444, -2e-8, 0.009421, -2.47e-5, 0.01098, -0.002563);
        path.cubic_rel(0.0123, -0.01953, 0.03278, -0.05074, 0.08648, -0.07769);
        path.cubic_rel(0.04491, -0.02187, 0.09409, -0.02597, 0.1246, -0.02636);
        path.line_rel(linear_section_length, 0.0);
        path.cubic_rel(0.05077, 0.0, 0.1629, -0.02346, 0.2307, -0.1455);
        path.line_rel(0.0, -0.007762);
        path.cubic_rel(-1.78e-6, -0.0125, -0.0006365, -0.01311, -0.01216, -0.01311);
        path.cubic_rel(
            -0.006444, -3.919e-8, -0.009348, 2.448e-5, -0.01091, 0.002563,
        );
        path.cubic_rel(-0.0123, 0.01953, -0.03278, 0.05074, -0.08648, 0.07769);
        path.cubic_rel(-0.04491, 0.02187, -0.09416, 0.02597, -0.1246, 0.02636);
        path.line_rel(-linear_section_length, 0.0);
        path.cubic_rel(-0.04786, 0.0, -0.1502, 0.02094, -0.2185, 0.1256);
        path.cubic_rel(-0.06833, -0.1046, -0.1706, -0.1256, -0.2185, -0.1256);
        path.line_rel(-linear_section_length, 0.0);
        path.cubic_rel(-0.03046, -0.0003899, -0.07972, -0.004491, -0.1246, -0.02636);
        path.cubic_rel(-0.0537, -0.02695, -0.07418, -0.05816, -0.08648, -0.07769);
        path.cubic_rel(
            -0.001562, -0.002538, -0.004467, -0.002563, -0.01091, -0.002563,
        );
        path.commands.push(RawPathCommand::Close);
        path
    }

    fn move_to(&mut self, x: f64, y: f64) {
        self.current = Point64::new(x, y);
        self.commands.push(RawPathCommand::Move(self.current));
    }

    fn line_rel(&mut self, dx: f64, dy: f64) {
        self.current = self.current.offset(dx, dy);
        self.commands.push(RawPathCommand::Line(self.current));
    }

    #[allow(clippy::too_many_arguments)]
    fn cubic_rel(
        &mut self,
        control1_x: f64,
        control1_y: f64,
        control2_x: f64,
        control2_y: f64,
        to_x: f64,
        to_y: f64,
    ) {
        let start = self.current;
        let control1 = start.offset(control1_x, control1_y);
        let control2 = start.offset(control2_x, control2_y);
        let to = start.offset(to_x, to_y);
        self.commands.push(RawPathCommand::Cubic {
            control1,
            control2,
            to,
        });
        self.current = to;
    }

    fn bounds(&self) -> Bounds2D64 {
        let mut bounds: Option<Bounds2D64> = None;
        let mut include = |point: Point64| {
            if let Some(bounds) = &mut bounds {
                bounds.include(point.x, point.y);
            } else {
                bounds = Some(Bounds2D64::point(point.x, point.y));
            }
        };
        for command in &self.commands {
            match *command {
                RawPathCommand::Move(point) | RawPathCommand::Line(point) => include(point),
                RawPathCommand::Cubic {
                    control1,
                    control2,
                    to,
                } => {
                    include(control1);
                    include(control2);
                    include(to);
                }
                RawPathCommand::Close => {}
            }
        }
        bounds.expect("Brace template always contains points")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MobjectTarget, DOWN, RIGHT};

    const EPSILON: f64 = 2.0e-5;

    #[test]
    fn brace_matches_default_square_width_and_buffer_without_mutating_target() {
        let scene = Scene::new();
        let square = scene.square(2.0).unwrap();
        let before = square.state().unwrap();

        let brace = scene
            .brace(
                &LayoutAnchor::from(&square),
                (DOWN.x as f64, DOWN.y as f64),
                0.2,
                2.0,
            )
            .unwrap();

        assert_eq!(square.state().unwrap(), before);
        assert!((brace.width().unwrap() - 2.0).abs() < EPSILON);
        let target = square.layout_bounds().unwrap().unwrap();
        let brace_bounds = brace.layout_bounds().unwrap().unwrap();
        assert!((brace_bounds.max_y - (target.min_y - 0.2)).abs() < EPSILON);
        assert_eq!(brace.wire_stroke_width().unwrap(), 0.0);
        assert!(brace.wire_fill().unwrap().is_some());
    }

    #[test]
    fn brace_observes_family_bounds_once_without_changing_members() {
        let scene = Scene::new();
        let mut left = scene.square(1.0).unwrap();
        let mut right = scene.square(1.0).unwrap();
        left.shift(-1.0, 0.0).unwrap();
        right.shift(1.0, 0.0).unwrap();
        let family = scene
            .family(&[MobjectTarget::Object(&left), MobjectTarget::Object(&right)])
            .unwrap();
        let left_before = left.state().unwrap();
        let right_before = right.state().unwrap();

        let brace = scene
            .brace(
                &LayoutAnchor::from(&family),
                (DOWN.x as f64, DOWN.y as f64),
                0.2,
                2.0,
            )
            .unwrap();

        assert_eq!(left.state().unwrap(), left_before);
        assert_eq!(right.state().unwrap(), right_before);
        assert!((brace.width().unwrap() - 3.0).abs() < EPSILON);
    }

    #[test]
    fn brace_between_points_auto_direction_matches_line_normal() {
        let scene = Scene::new();
        let brace = scene
            .brace_between_points((-1.0, 0.0), (1.0, 0.0), (0.0, 0.0), 0.2, 2.0)
            .unwrap();
        let bounds = brace.layout_bounds().unwrap().unwrap();
        assert!((bounds.max_y + 0.2).abs() < EPSILON);
        assert!((brace.width().unwrap() - 2.0).abs() < EPSILON);
    }

    #[test]
    fn right_facing_brace_uses_projected_target_extent() {
        let scene = Scene::new();
        let target = scene.rectangle(2.0, 3.0).unwrap();
        let brace = scene
            .brace(
                &LayoutAnchor::from(&target),
                (RIGHT.x as f64, RIGHT.y as f64),
                0.2,
                2.0,
            )
            .unwrap();
        assert!((brace.height().unwrap() - 3.0).abs() < EPSILON);
    }

    #[test]
    fn brace_rejects_non_finite_inputs_before_object_creation() {
        let scene = Scene::new();
        let square = scene.square(2.0).unwrap();
        let revision = scene.revision();
        let error = scene
            .brace(&LayoutAnchor::from(&square), (0.0, f64::NAN), 0.2, 2.0)
            .unwrap_err();
        assert!(matches!(error, AuthoringError::InvalidRenderNumber { .. }));
        assert_eq!(scene.revision(), revision);
    }

    #[test]
    fn brace_rejects_foreign_target_without_observing_it() {
        let scene = Scene::new();
        let other = Scene::new();
        let target = other.square(1.0).unwrap();
        assert_eq!(
            scene
                .brace(&LayoutAnchor::from(&target), (0.0, -1.0), 0.2, 2.0)
                .unwrap_err(),
            AuthoringError::ForeignStore
        );
    }
}
