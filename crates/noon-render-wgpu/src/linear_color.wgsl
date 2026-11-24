// Linear alpha-associated radiance for the finite RGBA16Float scene target.
// Geometry programs share this private helper entry with the compatibility
// specialization. Preserve energy above one; associate alpha exactly once.
// The caller supplies finite, nonnegative, unassociated linear RGB.
fn cairo_source_color(color: vec4<f32>, opacity: f32) -> vec4<f32> {
    let alpha = clamp(color.a * opacity, 0.0, 1.0);
    return vec4<f32>(color.rgb * alpha, alpha);
}
