//! Shared object alignment and relative placement.
use super::*;
impl Mobject {
    pub fn manim_move_to_handle(
        &mut self,
        other: &Self,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        self.validate()?;
        let edge = authoring_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = authoring_xy_f64(mask_x, mask_y)?;
        let source = self.critical_point(edge.x, edge.y)?;
        let target = other.critical_point(edge.x, edge.y)?;
        self.shift(
            (target.0 - source.0) * mask.x,
            (target.1 - source.1) * mask.y,
        )
    }
    pub fn manim_move_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        self.validate()?;
        let point = authoring_xy_f64(point_x, point_y)?;
        let edge = authoring_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = authoring_xy_f64(mask_x, mask_y)?;
        let source = self.critical_point(edge.x, edge.y)?;
        self.shift((point.x - source.0) * mask.x, (point.y - source.1) * mask.y)
    }
    pub fn manim_next_to_handle(
        &mut self,
        other: &Self,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        self.validate()?;
        let direction = authoring_xy_f64(args.direction.0, args.direction.1)?;
        let edge = authoring_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = authoring_xy_f64(args.mask.0, args.mask.1)?;
        let buff = authoring_render_f64("buffer", args.buff)?;
        let source = self.critical_point(edge.x - direction.x, edge.y - direction.y)?;
        let target = other.critical_point(edge.x + direction.x, edge.y + direction.y)?;
        self.shift(
            (target.0 - source.0 + direction.x * buff) * mask.x,
            (target.1 - source.1 + direction.y * buff) * mask.y,
        )
    }
    pub fn manim_next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        self.validate()?;
        let point = authoring_xy_f64(point_x, point_y)?;
        let direction = authoring_xy_f64(args.direction.0, args.direction.1)?;
        let edge = authoring_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = authoring_xy_f64(args.mask.0, args.mask.1)?;
        let buff = authoring_render_f64("buffer", args.buff)?;
        let source = self.critical_point(edge.x - direction.x, edge.y - direction.y)?;
        self.shift(
            (point.x - source.0 + direction.x * buff) * mask.x,
            (point.y - source.1 + direction.y * buff) * mask.y,
        )
    }
    pub fn next_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        self.validate()?;
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y)?;
        let target = other.critical_point(axis_x, axis_y)?;
        self.shift(
            target.0 - source.0 + axis_x * buff,
            target.1 - source.1 + axis_y * buff,
        )
    }
    pub fn next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        self.validate()?;
        semantic_xy(point_x, point_y)?;
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y)?;
        self.shift(
            point_x - source.0 + axis_x * buff,
            point_y - source.1 + axis_y * buff,
        )
    }
    pub fn align_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        self.validate()?;
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let source = self.critical_point(direction_x, direction_y)?;
        let target = other.critical_point(direction_x, direction_y)?;
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                target.0 - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                target.1 - source.1
            },
        )
    }
    pub fn align_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        self.validate()?;
        semantic_xy(point_x, point_y)?;
        let source = self.critical_point(direction_x, direction_y)?;
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                point_x - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                point_y - source.1
            },
        )
    }
    pub fn align_on_frame(
        &mut self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        self.validate()?;
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let point = self.critical_point(direction_x, direction_y)?;
        let mut shift_x = 0.0;
        let mut shift_y = 0.0;
        if direction_x != 0.0 {
            let target = direction_x.signum() * f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5;
            shift_x = target - point.0 - direction_x * buff;
        }
        if direction_y != 0.0 {
            let target = direction_y.signum() * f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5;
            shift_y = target - point.1 - direction_y * buff;
        }
        self.shift(shift_x, shift_y)
    }
}
