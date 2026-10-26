// Rasterizer vertex snapping can shift interpolated local coordinates across
// nearest-texel boundaries. Reconstruct paint placement from the pixel position;
// the existing interpolated geometry still owns coverage and stroke.
@fragment
fn fs_image_paint_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = image_paint_fill(image_paint_local_from_pixel(input.position.xy), input.fill);
    return circle_color(painted);
}

@fragment
fn fs_image_paint_rectangle(input: RectangleVertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = image_paint_fill(image_paint_local_from_pixel(input.position.xy), input.fill);
    return rectangle_color(painted);
}
