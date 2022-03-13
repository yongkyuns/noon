use crate::Interpolate;
use nannou::color::{rgb_u32, rgba, Rgb};
use nannou::rand::{prelude::SliceRandom, thread_rng};
use std::marker::PhantomData;

pub type Color = nannou::color::Rgb;

impl Interpolate for Color {
    fn interp(&self, other: &Self, progress: f32) -> Self {
        let progress = progress.min(1.0).max(0.0);
        Self {
            red: self.red.interp(&other.red, progress),
            green: self.green.interp(&other.green, progress),
            blue: self.blue.interp(&other.blue, progress),
            standard: PhantomData,
        }
    }
}

impl ColorExtension for Color {
    fn get_color(&self) -> Color {
        *self
    }
}

pub trait ColorExtension {
    const WHITE: Color = Color {
        red: 255.0 / 255.0,
        green: 255.0 / 255.0,
        blue: 255.0 / 255.0,
        standard: PhantomData,
    };
    const BLACK: Color = Color {
        red: 0.0 / 255.0,
        green: 0.0 / 255.0,
        blue: 0.0 / 255.0,
        standard: PhantomData,
    };
    const RED: Color = Color {
        red: 255.0 / 255.0,
        green: 0.0 / 255.0,
        blue: 0.0 / 255.0,
        standard: PhantomData,
    };
    const BLUE: Color = Color {
        red: 0.0 / 255.0,
        green: 0.0 / 255.0,
        blue: 255.0 / 255.0,
        standard: PhantomData,
    };

    fn palette() -> [Color; 5] {
        [
            rgb_from_hex(0x264653),
            rgb_from_hex(0x2a9d8f),
            rgb_from_hex(0xe9c46a),
            rgb_from_hex(0xf4a261),
            rgb_from_hex(0xe76f51),
        ]
    }
    fn random() -> Color {
        *Self::palette().choose(&mut thread_rng()).unwrap()
    }
    fn get_color(&self) -> Color;
    fn brighten(&self) -> Color {
        let mut hsv: nannou::color::Hsv = self.get_color().into_linear().into();
        hsv.saturation -= 0.1;
        hsv.value += 0.2;
        hsv.into()
    }
}

pub fn rgb_from_hex(color: u32) -> Rgb {
    let color = rgb_u32(color);
    rgba(
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        1.0,
    )
    .into()
}
