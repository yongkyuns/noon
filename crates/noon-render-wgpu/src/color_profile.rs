//! Derived working-color conversion before association and raster blending.
use noon_core::{Color, ViewColorProfile};

use crate::{CircleInstance, LineInstance, PackedStyle, PathInstance, RectangleInstance};

fn decode(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn convert(color: &mut [f32; 4], profile: ViewColorProfile) {
    if profile == ViewColorProfile::LinearHdr {
        for channel in &mut color[..3] {
            *channel = decode(*channel);
        }
    }
}

pub(crate) fn authored_color(color: Color, profile: ViewColorProfile) -> Color {
    let mut channels = [color.red, color.green, color.blue, color.alpha];
    convert(&mut channels, profile);
    Color {
        red: channels[0],
        green: channels[1],
        blue: channels[2],
        alpha: channels[3],
    }
}

pub(crate) trait WithColorProfile {
    fn with_color_profile(self, profile: ViewColorProfile) -> Self;
}

impl WithColorProfile for PackedStyle {
    fn with_color_profile(mut self, profile: ViewColorProfile) -> Self {
        convert(&mut self.fill, profile);
        convert(&mut self.stroke, profile);
        self
    }
}

macro_rules! analytic {
    ($($instance:ty),*) => {$(
        impl WithColorProfile for $instance {
            fn with_color_profile(mut self, profile: ViewColorProfile) -> Self {
                self.style = self.style.with_color_profile(profile);
                self
            }
        }
    )*};
}
analytic!(CircleInstance, RectangleInstance, LineInstance);

impl WithColorProfile for PathInstance {
    fn with_color_profile(mut self, profile: ViewColorProfile) -> Self {
        self.style = self.style.with_color_profile(profile);
        convert(&mut self.background_stroke, profile);
        self
    }
}
