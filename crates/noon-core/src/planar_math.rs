//! Planar values with explicit semantic and renderer precision.
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

macro_rules! planar_vector {
    ($name:ident, $scalar:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub x: $scalar,
            pub y: $scalar,
        }

        impl $name {
            pub const ZERO: Self = Self::new(0.0, 0.0);
            pub const ONE: Self = Self::new(1.0, 1.0);

            pub const fn new(x: $scalar, y: $scalar) -> Self {
                Self { x, y }
            }

            pub fn length(self) -> $scalar {
                self.x.hypot(self.y)
            }

            pub fn normalized(self) -> Option<Self> {
                let length = self.length();
                (length > 0.0 && length.is_finite()).then(|| self / length)
            }

            pub fn component_mul(self, rhs: Self) -> Self {
                Self::new(self.x * rhs.x, self.y * rhs.y)
            }

            pub fn rotate(self, angle: $scalar) -> Self {
                let (sin, cos) = angle.sin_cos();
                Self::new(self.x * cos - self.y * sin, self.x * sin + self.y * cos)
            }
        }

        impl Add for $name {
            type Output = Self;

            fn add(self, rhs: Self) -> Self::Output {
                Self::new(self.x + rhs.x, self.y + rhs.y)
            }
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, rhs: Self) {
                *self = *self + rhs;
            }
        }

        impl Sub for $name {
            type Output = Self;

            fn sub(self, rhs: Self) -> Self::Output {
                Self::new(self.x - rhs.x, self.y - rhs.y)
            }
        }

        impl SubAssign for $name {
            fn sub_assign(&mut self, rhs: Self) {
                *self = *self - rhs;
            }
        }

        impl Neg for $name {
            type Output = Self;

            fn neg(self) -> Self::Output {
                Self::new(-self.x, -self.y)
            }
        }

        impl Mul<$scalar> for $name {
            type Output = Self;

            fn mul(self, rhs: $scalar) -> Self::Output {
                Self::new(self.x * rhs, self.y * rhs)
            }
        }

        impl Mul<$name> for $scalar {
            type Output = $name;

            fn mul(self, rhs: $name) -> Self::Output {
                rhs * self
            }
        }

        impl Div<$scalar> for $name {
            type Output = Self;

            fn div(self, rhs: $scalar) -> Self::Output {
                Self::new(self.x / rhs, self.y / rhs)
            }
        }
    };
}

macro_rules! planar_rectangle {
    ($name:ident, $vector:ident, $scalar:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            pub min: $vector,
            pub max: $vector,
        }

        impl $name {
            pub const fn new(min: $vector, max: $vector) -> Self {
                Self { min, max }
            }

            pub fn from_points(points: impl IntoIterator<Item = $vector>) -> Option<Self> {
                let mut points = points.into_iter();
                let first = points.next()?;
                let mut result = Self::new(first, first);
                for point in points {
                    result.include(point);
                }
                Some(result)
            }

            pub fn include(&mut self, point: $vector) {
                self.min.x = self.min.x.min(point.x);
                self.min.y = self.min.y.min(point.y);
                self.max.x = self.max.x.max(point.x);
                self.max.y = self.max.y.max(point.y);
            }

            pub fn union(self, other: Self) -> Self {
                Self::new(
                    $vector::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
                    $vector::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
                )
            }

            pub fn center(self) -> $vector {
                (self.min + self.max) * 0.5
            }

            pub fn size(self) -> $vector {
                self.max - self.min
            }

            pub fn width(self) -> $scalar {
                self.max.x - self.min.x
            }

            pub fn height(self) -> $scalar {
                self.max.y - self.min.y
            }

            pub fn critical_point(self, direction: $vector) -> $vector {
                $vector::new(
                    if direction.x < 0.0 {
                        self.min.x
                    } else if direction.x > 0.0 {
                        self.max.x
                    } else {
                        self.center().x
                    },
                    if direction.y < 0.0 {
                        self.min.y
                    } else if direction.y > 0.0 {
                        self.max.y
                    } else {
                        self.center().y
                    },
                )
            }
        }
    };
}

planar_vector!(Vec2, f32);
planar_vector!(DVec2, f64);
planar_rectangle!(Rect, Vec2, f32);
planar_rectangle!(DRect, DVec2, f64);

impl From<Vec2> for DVec2 {
    fn from(value: Vec2) -> Self {
        Self::new(f64::from(value.x), f64::from(value.y))
    }
}

impl DVec2 {
    /// Explicit compact execution/render projection; semantic values stay f64.
    pub fn lower_f32(self) -> Result<Vec2, crate::SemanticLoweringError> {
        crate::SemanticVec3::new(self.x, self.y, 0.0).lower_xy_f32()
    }
}

impl From<Rect> for DRect {
    fn from(value: Rect) -> Self {
        Self::new(value.min.into(), value.max.into())
    }
}

impl DRect {
    pub fn lower_f32(self) -> Result<Rect, crate::SemanticLoweringError> {
        Ok(Rect::new(self.min.lower_f32()?, self.max.lower_f32()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_values_preserve_precision_until_explicit_render_projection() {
        let bounds = DRect::new(DVec2::new(1.0, -2.0), DVec2::new(1.0 + 1e-10, 3.0));
        assert!(bounds.width() > 0.0);
        assert_eq!(bounds.lower_f32().unwrap().width(), 0.0);
        let outside_render_range = DVec2::new(1e100, 0.0);
        assert!(outside_render_range.x.is_finite());
        assert!(outside_render_range.lower_f32().is_err());
        assert!(DVec2::new(f64::NAN, 0.0).lower_f32().is_err());
    }

    #[test]
    fn compact_layout_stays_unchanged() {
        assert_eq!(std::mem::size_of::<Vec2>(), 8);
        assert_eq!(std::mem::size_of::<Rect>(), 16);
        assert_eq!(std::mem::size_of::<DVec2>(), 16);
        assert_eq!(std::mem::size_of::<DRect>(), 32);
    }
}
