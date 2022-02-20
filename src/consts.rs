use std::marker::PhantomData;

use crate::Color;

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
