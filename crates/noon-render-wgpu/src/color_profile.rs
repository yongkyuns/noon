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

fn convert(color: &mut [f32; 4], profile: ViewColorProfile, emission: noon_core::Emission) {
    if profile == ViewColorProfile::LinearHdr {
        for channel in &mut color[..3] {
            *channel = decode(*channel) * emission.radiance_multiplier() as f32;
        }
    }
}

pub(crate) fn authored_color(color: Color, profile: ViewColorProfile) -> Color {
    let mut channels = [color.red, color.green, color.blue, color.alpha];
    convert(&mut channels, profile, noon_core::Emission::NONE);
    Color {
        red: channels[0],
        green: channels[1],
        blue: channels[2],
        alpha: channels[3],
    }
}

pub(crate) trait WithColorProfile {
    fn with_color_profile(self, profile: ViewColorProfile, emission: noon_core::Emission) -> Self;
}

impl WithColorProfile for PackedStyle {
    fn with_color_profile(
        mut self,
        profile: ViewColorProfile,
        emission: noon_core::Emission,
    ) -> Self {
        convert(&mut self.fill, profile, emission);
        convert(&mut self.stroke, profile, emission);
        self
    }
}

macro_rules! analytic {
    ($($instance:ty),*) => {$(
        impl WithColorProfile for $instance {
            fn with_color_profile(mut self, profile: ViewColorProfile, emission: noon_core::Emission) -> Self {
                self.style = self.style.with_color_profile(profile, emission);
                self
            }
        }
    )*};
}
analytic!(CircleInstance, RectangleInstance, LineInstance);

impl WithColorProfile for PathInstance {
    fn with_color_profile(
        mut self,
        profile: ViewColorProfile,
        emission: noon_core::Emission,
    ) -> Self {
        self.style = self.style.with_color_profile(profile, emission);
        convert(&mut self.background_stroke, profile, emission);
        self
    }
}

/// Asset texels and glyph tints are authored sRGB; decode at the draw boundary
/// before association/filtering. Float target geometry is already CPU-decoded.
pub(crate) fn working_color_shader(format: wgpu::TextureFormat) -> String {
    format!(
        "const LINEAR_WORKING_COLOR: bool = {};\n{}",
        format == wgpu::TextureFormat::Rgba16Float,
        include_str!("working_color.wgsl")
    )
}

/// Convert an authored sRGB clear into the selected renderer working profile.
/// Alpha stays linear; the GPU receives already-decoded color in an HDR view.
pub fn authored_clear_color(color: wgpu::Color, profile: ViewColorProfile) -> wgpu::Color {
    if profile == ViewColorProfile::Compatibility {
        return color;
    }
    let decode = |x: f64| {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    };
    wgpu::Color {
        r: decode(color.r),
        g: decode(color.g),
        b: decode(color.b),
        a: color.a,
    }
}
