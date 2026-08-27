struct CameraUniform {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@group(1) @binding(0)
var glyph_atlas: texture_2d<f32>;

@group(1) @binding(1)
var glyph_sampler: sampler;

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
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
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
    // Atlas rows are uploaded top-to-bottom while glyph quads are stored bottom-left
    // with a Y-up axis. Flip only the texture V coordinate here.
    output.uv = vec2<f32>(
        mix(input.uv_min.x, input.uv_max.x, corner.x),
        mix(input.uv_max.y, input.uv_min.y, corner.y),
    );
    output.color = input.color;
    return output;
}

@fragment
fn fs_mask(input: GlyphVertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(glyph_atlas, glyph_sampler, input.uv).r;
    let alpha = coverage * input.color.a;
    return vec4<f32>(input.color.rgb * alpha, alpha);
}

@fragment
fn fs_color(input: GlyphVertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(glyph_atlas, glyph_sampler, input.uv);
    let alpha = sampled.a * input.color.a;
    return vec4<f32>(sampled.rgb * input.color.rgb * alpha, alpha);
}
