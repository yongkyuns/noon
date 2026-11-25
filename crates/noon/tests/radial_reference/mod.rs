//! Independent scalar radial specification at physical pixel centers. The
//! reference uses authored style/bounds, never renderer uniforms or mask output.
#[derive(Clone, Copy)]
pub struct Profile {
    pub center: [f64; 2],
    pub axes: [f64; 2],
    pub softness: f64,
    pub seed: u32,
    pub roughness: f64,
    pub frequency: f64,
    pub rim_width: f64,
    pub inner: [f64; 4],
    pub outer: [f64; 4],
    pub hole: bool,
}
impl Profile {
    pub fn new(burn: bool, hole: bool) -> Self {
        Self {
            center: [0.4, 0.6],
            axes: [0.55, 0.4],
            softness: if burn { 0.025 } else { 0.0 },
            seed: 7,
            roughness: if burn { 0.1 } else { 0.0 },
            frequency: 12.0,
            rim_width: if burn { 0.12 } else { 0.0 },
            inner: [1.0, 0.85, 0.24, 1.0],
            outer: [0.24, 0.035, 0.01, 0.7],
            hole,
        }
    }
}
fn hash(seed: u32, x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ seed;
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
    h ^ (h >> 16)
}
fn noise(seed: u32, point: [f64; 2]) -> f64 {
    let base = point.map(|v| v.floor() as i32);
    let t = point.map(|v| {
        let f = v - v.floor();
        f * f * (3.0 - 2.0 * f)
    });
    let mut result = 0.0;
    for y in 0..2 {
        for x in 0..2 {
            result += f64::from(hash(seed, base[0] + x, base[1] + y) >> 8) / 16777216.0
                * if x == 0 { 1.0 - t[0] } else { t[0] }
                * if y == 0 { 1.0 - t[1] } else { t[1] };
        }
    }
    result
}
fn smooth(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
pub fn sample(
    source: [f64; 4],
    uv: [f64; 2],
    dimensions: [f64; 2],
    progress: f64,
    p: Profile,
) -> [f64; 4] {
    if progress <= 0.0 {
        return source;
    }
    if progress >= 1.0 {
        return [0.0; 4];
    }
    let q = std::array::from_fn::<_, 2, _>(|i| (uv[i] - p.center[i]) / p.axes[i]);
    let furthest =
        std::array::from_fn::<_, 2, _>(|i| p.center[i].max(1.0 - p.center[i]) / p.axes[i]);
    let maximum = furthest[0].hypot(furthest[1]);
    let footprint = (1.0 / dimensions[0] / p.axes[0]).hypot(1.0 / dimensions[1] / p.axes[1]);
    let unresolved = (1.0 / dimensions[0]).max(1.0 / dimensions[1]) * p.frequency;
    let roughness = p.roughness * (1.0 - smooth(0.5, 1.0, unresolved));
    let distance =
        q[0].hypot(q[1]) + roughness * (2.0 * noise(p.seed, uv.map(|v| v * p.frequency)) - 1.0);
    let width = p.softness.max(footprint);
    let margin = p.roughness + width + p.rim_width;
    let opening = if p.hole { progress } else { 1.0 - progress };
    let radius = -margin + (maximum + 2.0 * margin) * opening;
    let signed = if p.hole {
        distance - radius
    } else {
        radius - distance
    };
    let coverage = smooth(-0.5 * width, 0.5 * width, signed);
    let mut color = source;
    if p.rim_width > 0.0 {
        let phase = (signed / p.rim_width).clamp(0.0, 1.0);
        let alpha = p.inner[3] * (1.0 - phase) + p.outer[3] * phase;
        let band = 1.0
            - smooth(
                (p.rim_width - footprint).max(0.0),
                p.rim_width + footprint,
                signed,
            );
        for i in 0..3 {
            let rim = p.inner[i] * p.inner[3] * (1.0 - phase) + p.outer[i] * p.outer[3] * phase;
            color[i] = source[i] * (1.0 - band * alpha) + rim * source[3] * band;
        }
    }
    color.map(|v| v * coverage)
}
