@fragment
fn fs_paint_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    var local = input.local;
    // Keep the hatch footprint centered on the actual physical pixel, rather
    // than a varying whose vertices may have been snapped by the rasterizer.
    if paint.metadata.y == 4u { local = paint_local_from_pixel(input.position.xy); }
    painted.fill = paint_fill(local, input.fill);
    return circle_color(painted);
}

@fragment
fn fs_paint_rectangle(input: RectangleVertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    var local = input.local;
    if paint.metadata.y == 4u { local = paint_local_from_pixel(input.position.xy); }
    painted.fill = paint_fill(local, input.fill);
    return rectangle_color(painted);
}
