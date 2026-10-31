// Exact box coverage of a vertical band in affine local coordinates.
fn scan_cdf(t: f32, gradient: vec2<f32>) -> f32 {
    let a = max(abs(gradient.x), abs(gradient.y));
    let b = min(abs(gradient.x), abs(gradient.y));
    if a == 0.0 { return select(0.0, 1.0, t >= 0.0); }
    if b == 0.0 { return clamp(t / a + 0.5, 0.0, 1.0); }
    let s = clamp(t + 0.5 * (a + b), 0.0, a + b);
    if s < b { return 0.5 * s * s / (a * b); }
    if s <= a { return (s - 0.5 * b) / a; }
    let tail = a + b - s;
    return 1.0 - 0.5 * tail * tail / (a * b);
}
fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {
    let gradient = vec2<f32>(dpdx(input.position.x), dpdy(input.position.x));
    let offset = p.phase - input.position.x;
    let coverage = scan_cdf(offset + 0.5 * p.width, gradient)
                 - scan_cdf(offset - 0.5 * p.width, gradient);
    return vec4<f32>(1.0, 1.0, 0.0, clamp(coverage, 0.0, 1.0));
}
