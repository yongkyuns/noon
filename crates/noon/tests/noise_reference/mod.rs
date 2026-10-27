//! Independent specification oracle: masked integer arithmetic and piecewise
//! Gaussian quadrature, without the production sampler/antiderivative/uniforms.
fn hash(seed: u32, x: i64, y: i64) -> f64 {
    let mask = 0xffff_ffff_u64;
    let mut n = u64::from(seed)
        ^ ((x.rem_euclid(65_536) as u64 * 0x9e37_79b9) & mask)
        ^ ((y.rem_euclid(65_536) as u64 * 0x85eb_ca6b) & mask);
    for (shift, factor) in [(16, 0x7feb_352d), (15, 0x846c_a68b)] {
        n = ((n ^ (n >> shift)) * factor) & mask;
    }
    n ^= n >> 16;
    (n >> 16) as f64 / 65_535.0
}

fn point(seed: u32, x: f64, y: f64) -> f64 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let curve = |v: f64| 10.0 * v.powi(3) - 15.0 * v.powi(4) + 6.0 * v.powi(5);
    let u = curve(x - x.floor());
    let v = curve(y - y.floor());
    (1.0 - u) * (1.0 - v) * hash(seed, ix, iy)
        + u * (1.0 - v) * hash(seed, ix + 1, iy)
        + (1.0 - u) * v * hash(seed, ix, iy + 1)
        + u * v * hash(seed, ix + 1, iy + 1)
}

fn quadrature(center: f64, width: f64) -> Vec<(f64, f64)> {
    if width < 1.0 / 256.0 {
        return vec![(center, 1.0)];
    }
    let a = center - width / 2.0;
    let b = center + width / 2.0;
    let mut cuts = vec![a];
    let split = a.floor() + 1.0;
    if split < b {
        cuts.push(split);
    }
    cuts.push(b);
    let mut result = Vec::new();
    let root = (3.0_f64 / 5.0).sqrt();
    for segment in cuts.windows(2) {
        let half = (segment[1] - segment[0]) / 2.0;
        let mid = (segment[1] + segment[0]) / 2.0;
        for (node, weight) in [(-root, 5.0 / 9.0), (0.0, 8.0 / 9.0), (root, 5.0 / 9.0)] {
            result.push((mid + node * half, weight * half / width));
        }
    }
    result
}

pub fn filtered(
    seed: u32,
    uv: [f64; 2],
    frequency: [f64; 2],
    phase: [f64; 2],
    widths: [f64; 2],
) -> f64 {
    let coarse = widths[0].max(widths[1]);
    if coarse >= 2.0 {
        return 0.5;
    }
    let q: [f64; 2] = std::array::from_fn(|i| uv[i].clamp(0.0, 1.0) * frequency[i] + phase[i]);
    let mut value = 0.0;
    for (x, wx) in quadrature(q[0], widths[0].min(1.0)) {
        for (y, wy) in quadrature(q[1], widths[1].min(1.0)) {
            value += wx * wy * point(seed, x, y);
        }
    }
    let a = (coarse - 1.0).clamp(0.0, 1.0);
    let fade = 3.0 * a * a - 2.0 * a * a * a;
    value * (1.0 - fade) + 0.5 * fade
}
