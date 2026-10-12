struct CameraUniform {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var glyph_atlas: texture_2d<f32>;

struct GlyphVertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) origin: vec2<f32>,
    @location(1) axis_x: vec2<f32>,
    @location(2) axis_y: vec2<f32>,
    @location(3) uv_min: vec2<f32>,
    @location(4) uv_max: vec2<f32>,
    @location(5) color: vec4<f32>,
};

struct GlyphVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) atlas_uv_min: vec2<f32>,
    @location(3) @interpolate(flat) atlas_uv_max: vec2<f32>,
};

@vertex
fn vs_glyph(input: GlyphVertexInput) -> GlyphVertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );
    let corner = corners[input.vertex_index];
    let world = input.origin + input.axis_x * corner.x + input.axis_y * corner.y;

    var output: GlyphVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);
    // Interpolate only glyph-local coordinates. Interpolating absolute normalized
    // atlas coordinates makes the filter fraction depend on shelf placement through
    // f32 rounding, so identical final frames can differ by playback allocation history.
    output.local_uv = vec2<f32>(corner.x, 1.0 - corner.y);
    output.atlas_uv_min = input.uv_min;
    output.atlas_uv_max = input.uv_max;
    output.color = input.color;
    return output;
}

fn sample_glyph(input: GlyphVertexOutput) -> vec4<f32> {
    // Apply the linear sampler bilinear convention, including its half-texel offset. The atlas
    // gutter guarantees all four loads are resident even on the visible mask edges.
    let atlas_dimensions = vec2<f32>(textureDimensions(glyph_atlas));
    let atlas_origin = vec2<u32>(round(input.atlas_uv_min * atlas_dimensions));
    let atlas_size = vec2<u32>(round((input.atlas_uv_max - input.atlas_uv_min) * atlas_dimensions));
    let local_texel = input.local_uv * vec2<f32>(atlas_size) - vec2<f32>(0.5);
    let local_base = vec2<i32>(floor(local_texel));
    let weight = fract(local_texel);
    let atlas_base = vec2<i32>(atlas_origin) + local_base;
    let top_left = textureLoad(glyph_atlas, atlas_base, 0);
    let top_right = textureLoad(glyph_atlas, atlas_base + vec2<i32>(1, 0), 0);
    let bottom_left = textureLoad(glyph_atlas, atlas_base + vec2<i32>(0, 1), 0);
    let bottom_right = textureLoad(glyph_atlas, atlas_base + vec2<i32>(1, 1), 0);
    return mix(mix(top_left, top_right, weight.x), mix(bottom_left, bottom_right, weight.x), weight.y);
}

@fragment
fn fs_mask(input: GlyphVertexOutput) -> @location(0) vec4<f32> {
    let coverage = sample_glyph(input).r;
    let alpha = coverage * input.color.a;
    return vec4<f32>(input.color.rgb * alpha, alpha);
}

@fragment
fn fs_color(input: GlyphVertexOutput) -> @location(0) vec4<f32> {
    let sampled = sample_glyph(input);
    let alpha = sampled.a * input.color.a;
    return vec4<f32>(sampled.rgb * input.color.rgb * alpha, alpha);
}
