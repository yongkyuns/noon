use crate::{Interpolate, Path, Position, TO_PXL};
use bevy_ecs::prelude::Component;
pub use nannou::lyon::math::{point, Point, Vector};
use std::{
    marker::PhantomData,
    ops::{Add, Mul},
};

#[derive(Component, Clone, Copy, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Trait for converting from native Noon scale into pixel scale
pub trait PixelFrame {
    fn into_pxl_scale(&self) -> Self;
    fn into_natural_scale(&self) -> Self;
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

impl PixelFrame for Point {
    fn into_pxl_scale(&self) -> Self {
        Self {
            x: self.x * TO_PXL,
            y: self.y * TO_PXL,
            _unit: PhantomData,
        }
    }
    fn into_natural_scale(&self) -> Self {
        Self {
            x: self.x / TO_PXL,
            y: self.y / TO_PXL,
            _unit: PhantomData,
        }
    }
}

/// Data type to represent physical size of any 2D object.
#[derive(Debug, Component, Clone, Copy)]
pub struct BoundingSize(pub(crate) Size);

impl BoundingSize {
    /// Update the bounding size of the [Path], when rotated by [Angle].
    pub fn from(path: &Path, angle: f32) -> BoundingSize {
        use nannou::lyon::algorithms::aabb::bounding_rect;
        let rotated = path
            .raw
            .clone()
            .transformed(&nannou::lyon::geom::Rotation::radians(angle));

        let rect = bounding_rect(rotated.iter());
        BoundingSize(Size::from(rect.width() / TO_PXL, rect.height() / TO_PXL))
    }
}

/// Data type to represent physical size of any 2D object.
#[derive(Debug, Component, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    /// Indicate size of zero (e.g. dot)
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };
    pub const UNIT: Self = Self {
        width: 1.0,
        height: 1.0,
    };

    /// Create size from radius, assuming circular shape
    pub fn from_radius(radius: f32) -> Self {
        Self {
            width: radius * 2.0,
            height: radius * 2.0,
        }
    }

    /// Constructor for size
    pub fn from(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn reduced_by(&self, other: &Size) -> Self {
        Self {
            width: (self.width - other.width).max(0.0),
            height: (self.height - other.height).max(0.0),
        }
    }

    /// Returns scale factor to the given input size. If the given
    /// input size is greater, scale factor will be greater than 1.
    pub fn scale_factor(&self, other: &Self) -> (f32, f32) {
        if self.width < std::f32::EPSILON {
            (1.0, other.height / self.height)
        } else if self.height < std::f32::EPSILON {
            (other.width / self.width, 1.0)
        } else {
            (other.width / self.width, other.height / self.height)
        }
    }

    /// Compute the size of the 2D bounding box for a given set
    /// of points.
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

impl PixelFrame for Size {
    fn into_pxl_scale(&self) -> Self {
        Self {
            width: self.width * TO_PXL,
            height: self.height * TO_PXL,
        }
    }
    fn into_natural_scale(&self) -> Self {
        Self {
            width: self.width / TO_PXL,
            height: self.height / TO_PXL,
        }
    }
}

impl Mul<Size> for Size {
    type Output = Self;
    fn mul(self, other: Size) -> Self::Output {
        Self {
            width: self.width * other.width,
            height: self.height * other.height,
        }
    }
}

impl Add<Size> for Size {
    type Output = Self;
    fn add(self, other: Size) -> Self::Output {
        Self {
            width: self.width + other.width,
            height: self.height + other.height,
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
    #[test]
    fn check_point_cmp() {
        let p1 = point(3.0, 1.0);
        let p2 = point(2.0, 4.0);

        assert_eq!(point(2.0, 1.0), p1.min(p2));
        assert_eq!(point(2.0, 1.0), p2.min(p1));
        assert_eq!(point(3.0, 4.0), p1.max(p2));
        assert_eq!(point(3.0, 4.0), p2.max(p1));
    }
}
