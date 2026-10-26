@fragment
fn fs_image_paint_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = image_paint_fill(input.local, input.fill);
    return circle_color(painted);
}

@fragment
fn fs_image_paint_rectangle(input: RectangleVertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = image_paint_fill(input.local, input.fill);
    return rectangle_color(painted);
}
