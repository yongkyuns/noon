use crate::Vector;

pub const PXL_WIDTH: u32 = 1920;
pub const PXL_HEIGHT: u32 = 1080;

/// Path flattenening tolerance for normal shapes under normal condition.
pub const EPS: f32 = 0.0001;

/// Path flattenening tolerance for interpolation and other
/// tasks where computation may be higher than usual.
pub const EPS_LOW: f32 = 0.001;

/// Scale conversion between noon and pixel coordinates
pub const TO_PXL: f32 = 200.0;
pub const ZOOM: f32 = 200.0;

pub const PI: f32 = std::f32::consts::PI;

pub const TAU: f32 = std::f32::consts::TAU;

pub const UP: Vector = Vector::new(0.0, 1.0);
pub const DOWN: Vector = Vector::new(0.0, -1.0);
pub const LEFT: Vector = Vector::new(-1.0, 0.0);
pub const RIGHT: Vector = Vector::new(1.0, 0.0);
