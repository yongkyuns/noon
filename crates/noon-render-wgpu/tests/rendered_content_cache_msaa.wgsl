// #2094 M0 operator probe only, not a production compositor or geometry oracle.
// Synthetic masks enumerate all 16 x 16 prefix/suffix coverage combinations.
// No claim about hardware sample locations is made.
@group(0) @binding(0) var stored: texture_multisampled_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(points[index], 0.0, 1.0);
}

// Byte-code witness: every alpha code occurs at every sample index, with
// distinct premultiplied channels. The Rust expectation uses integer bytes.
@fragment
fn seed(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    let x = u32(p.x);
    let a = (x + s * 37u + u32(p.y) * 13u) & 255u;
    return vec4<f32>(f32(a), f32(x & a), f32((s * 17u) % (a + 1u)), f32(a)) / 255.0;
}

@fragment
fn prefix(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    if (((u32(p.x) & 15u) >> s) & 1u) == 0u { discard; }
    let a = f32(1u + (u32(p.y) & 3u)) / 4.0;
    return vec4<f32>(a, 0.0, 0.0, a);
}

@fragment
fn suffix(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    if ((((u32(p.x) >> 4u) & 15u) >> s) & 1u) == 0u { discard; }
    let a = f32(1u + (u32(p.y) & 3u)) / 4.0;
    return vec4<f32>(0.0, 0.0, a, a);
}

@fragment
fn other_suffix(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    if (((((u32(p.x) >> 4u) & 15u) ^ 15u) >> s) & 1u) == 0u { discard; }
    return vec4<f32>(0.0, 0.5, 0.0, 0.5);
}

// Candidate operation: identical pixel/sample index, no blend/filter/transfer.
@fragment
fn restore(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    return textureLoad(stored, vec2<i32>(p.xy), i32(s));
}

// Deliberately broken restoration controls, never used in production.
@fragment
fn permuted(@builtin(position) p: vec4<f32>, @builtin(sample_index) s: u32)
    -> @location(0) vec4<f32> {
    return textureLoad(stored, vec2<i32>(p.xy), i32((s + 1u) & 3u));
}

@fragment
fn averaged(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let xy = vec2<i32>(p.xy);
    return (textureLoad(stored, xy, 0) + textureLoad(stored, xy, 1)
          + textureLoad(stored, xy, 2) + textureLoad(stored, xy, 3)) / 4.0;
}

// Independent address mapping for readback: four adjacent output pixels carry
// the four samples. This is not an MSAA resolve and does not call restore.
@fragment
fn extract(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let x = u32(p.x);
    return textureLoad(stored, vec2<i32>(i32(x / 4u), i32(p.y)), i32(x % 4u));
}
