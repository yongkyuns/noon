struct ImagePaintPathOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
    @location(3) is_stroke: f32,
    @location(4) @interpolate(flat) polygon_a: vec2<f32>,
    @location(5) @interpolate(flat) polygon_b: vec2<f32>,
    @location(6) @interpolate(flat) polygon_c: vec2<f32>,
    @location(7) @interpolate(flat) polygon_d: vec2<f32>,
    @location(8) @interpolate(flat) polygon_count: u32,
    @location(9) paint: vec2<f32>,
};

@vertex
fn vs_image_paint_path(input: PathVertexInput) -> ImagePaintPathOutput {
    let base = path_vertex(input);
    var output: ImagePaintPathOutput;
    output.position = base.position;
    output.color = base.color;
    output.path_progress = base.path_progress;
    output.reveal = base.reveal;
    output.is_stroke = base.is_stroke;
    output.polygon_a = base.polygon_a;
    output.polygon_b = base.polygon_b;
    output.polygon_c = base.polygon_c;
    output.polygon_d = base.polygon_d;
    output.polygon_count = base.polygon_count;
    let stroke = (input.surface_and_progress & 1u) != 0u;
    let background = (input.surface_and_progress & PATH_BACKGROUND_STROKE_FLAG) != 0u;
    let fill_enabled = (input.flags.x & 1u) != 0u;
    let derived_outline = base.reveal < 1.0 && fill_enabled && (input.flags.y & 1u) == 0u && !background;
    let painted = fill_enabled && !background && (!stroke || derived_outline);
    let outline_alpha = select(1.0, 1.0 - smoothstep(0.75, 1.0, base.reveal), stroke && derived_outline);
    output.paint = vec2<f32>(input.fill.a * input.metrics.y * outline_alpha, select(0.0, 1.0, painted));
    return output;
}

fn image_paint_path_input(input: ImagePaintPathOutput) -> PathVertexOutput {
    var base: PathVertexOutput;
    base.position = input.position;
    let color = image_paint_fill(image_paint_local_from_pixel(input.position.xy), vec4<f32>(1.0, 1.0, 1.0, input.paint.x));
    base.color = select(input.color, cairo_source_color(color, 1.0), input.paint.y > 0.5);
    base.path_progress = input.path_progress;
    base.reveal = input.reveal;
    base.is_stroke = input.is_stroke;
    base.polygon_a = input.polygon_a;
    base.polygon_b = input.polygon_b;
    base.polygon_c = input.polygon_c;
    base.polygon_d = input.polygon_d;
    base.polygon_count = input.polygon_count;
    return base;
}

@fragment
fn fs_image_paint_path(input: ImagePaintPathOutput) -> @location(0) vec4<f32> {
    return path_color(image_paint_path_input(input));
}
@fragment
fn fs_image_paint_path_stroke(input: ImagePaintPathOutput) -> @location(0) vec4<f32> {
    return path_stroke_color(image_paint_path_input(input));
}
