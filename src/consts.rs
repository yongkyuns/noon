use std::marker::PhantomData;

use crate::Color;

/// Path flattenening tolerance for normal shapes under normal condition.
pub const EPS: f32 = 0.01;

/// Path flattenening tolerance for interpolation and other
/// tasks where computation may be higher than usual.
pub const EPS_LOW: f32 = 0.3;

pub const WHITE: Color = Color {
    red: 255.0 / 255.0,
    green: 255.0 / 255.0,
    blue: 255.0 / 255.0,
    standard: PhantomData,
};

pub const BLACK: Color = Color {
    red: 0.0 / 255.0,
    green: 0.0 / 255.0,
    blue: 0.0 / 255.0,
    standard: PhantomData,
};

pub const RED: Color = Color {
    red: 255.0 / 255.0,
    green: 0.0 / 255.0,
    blue: 0.0 / 255.0,
    standard: PhantomData,
};

pub const BLUE: Color = Color {
    red: 0.0 / 255.0,
    green: 0.0 / 255.0,
    blue: 255.0 / 255.0,
    standard: PhantomData,
};
