use crate::{Interpolate, Position, TO_PXL};
use bevy_ecs::prelude::Component;
pub use nannou::lyon::math::{point, Point, Vector};
use std::{marker::PhantomData, ops::Mul};

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

#[derive(Debug, Component, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
    pub fn from_radius(radius: f32) -> Self {
        Self {
            width: radius * 2.0,
            height: radius * 2.0,
        }
    }

    pub fn from(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn from_points(points: &[Point]) -> Self {
        if !points.is_empty() {
            let mut min = *points.first().unwrap();
            let mut max = *points.first().unwrap();

            for &p in points.iter() {
                if p.x < min.x {
                    min.x = p.x;
                }
                if p.y < min.y {
                    min.y = p.y;
                }
                if p.x > max.x {
                    max.x = p.x;
                }
                if p.y > max.y {
                    max.y = p.y;
                }
            }
            Size {
                width: (max.x - min.x).abs(),
                height: (max.y - min.y).abs(),
            }
        } else {
            Size::ZERO
        }
    }
}

impl IntoPixelFrame for Size {
    fn into_pxl_scale(&self) -> Self {
        Self {
            width: self.width * TO_PXL,
            height: self.height * TO_PXL,
        }
    }
}

impl Mul<f32> for Size {
    type Output = Self;
    fn mul(self, value: f32) -> Self::Output {
        Self {
            width: self.width * value,
            height: self.height * value,
        }
    }
}

impl std::fmt::Display for Size {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(width:{:3.2}, height:{:3.2})", self.width, self.height)
    }
}

impl Interpolate for Size {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        Self {
            width: self.width.interp(&other.width, progress),
            height: self.height.interp(&other.height, progress),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn size_from_points() {
        let points = vec![point(2.0, 3.0), point(4.0, 1.0), point(8.0, -3.0)];
        assert_eq!(Size::from_points(&points).width, 6.0);
        assert_eq!(Size::from_points(&points).height, 6.0);
    }
}
