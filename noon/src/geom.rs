use crate::{Interpolate, Position, TO_PXL};
pub use nannou::lyon::math::{point, Point, Vector};
use std::marker::PhantomData;

/// Trait for converting from native Noon scale into pixel scale
pub trait IntoPixelFrame {
    fn into_pxl_scale(&self) -> Self;
}

impl Interpolate for Point {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        point(
            self.x.interp(&other.x, progress),
            self.y.interp(&other.y, progress),
        )
    }
}

impl Into<Position> for Point {
    fn into(self) -> Position {
        Position {
            x: self.x,
            y: self.y,
        }
    }
}

impl IntoPixelFrame for Point {
    fn into_pxl_scale(&self) -> Self {
        Self {
            x: self.x * TO_PXL,
            y: self.y * TO_PXL,
            _unit: PhantomData,
        }
    }
}
